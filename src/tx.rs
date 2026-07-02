use crate::storage::Value;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// 全局逻辑时钟, 单调递增
pub struct SCN(pub AtomicU64);

impl SCN {
    pub fn get(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
    pub fn next(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst) + 1
    }
}

/// Undo 记录: 用于事务回滚时逆序重放 (仅在内存中, 不持久化)
#[derive(Clone, Debug)]
pub enum UndoRecord {
    Insert { table_name: String, row_idx: usize },
}

/// Redo 记录: 用于崩溃恢复 (WAL), 序列化为 JSON 行写入 redo.log
/// 使用 internally tagged enum: {"type":"begin",...}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum RedoRecord {
    Begin { tx_id: u64, scn: u64 },
    Insert { tx_id: u64, scn: u64, table_name: String, values: Vec<Value>, row_idx: usize },
    Commit { tx_id: u64, scn: u64 },
    Rollback { tx_id: u64, scn: u64 },
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TxStatus {
    Active,
    Committed,
    RolledBack,
}

/// 事务管理器: 维护 SCN/事务状态/Undo 段/Redo 缓冲区
pub struct TransactionManager {
    pub scn: SCN,
    pub statuses: HashMap<u64, TxStatus>,
    /// 每个事务的 commit SCN, 用于可见性判断
    pub commit_scns: HashMap<u64, u64>,
    pub undo_segments: HashMap<u64, Vec<UndoRecord>>,
    pub redo_buffer: Vec<RedoRecord>,
    pub next_tx_id: AtomicU64,
    /// redo.log 文件句柄 (LGWR 刷盘目标)
    pub redo_file: Mutex<File>,
    /// 是否输出 trace 日志
    pub trace: bool,
}

impl TransactionManager {
    pub fn new(redo_path: &str, trace: bool) -> Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(redo_path)?;
        Ok(Self {
            scn: SCN(AtomicU64::new(0)),
            statuses: HashMap::new(),
            commit_scns: HashMap::new(),
            undo_segments: HashMap::new(),
            redo_buffer: Vec::new(),
            next_tx_id: AtomicU64::new(1),
            redo_file: Mutex::new(file),
            trace,
        })
    }

    pub fn begin(&mut self) -> u64 {
        let tx_id = self.next_tx_id.fetch_add(1, Ordering::SeqCst);
        let scn = self.scn.get();
        self.statuses.insert(tx_id, TxStatus::Active);
        self.undo_segments.insert(tx_id, Vec::new());
        self.redo_buffer.push(RedoRecord::Begin { tx_id, scn });
        if self.trace {
            println!("[TX] BEGIN tx_id={} scn={}", tx_id, scn);
        }
        tx_id
    }

    /// 记录一次 INSERT, 同时生成 Undo + Redo
    pub fn record_insert(
        &mut self,
        tx_id: u64,
        table_name: &str,
        values: Vec<Value>,
        row_idx: usize,
    ) {
        let scn = self.scn.get();
        self.undo_segments.entry(tx_id).or_default().push(UndoRecord::Insert {
            table_name: table_name.to_string(),
            row_idx,
        });
        self.redo_buffer.push(RedoRecord::Insert {
            tx_id,
            scn,
            table_name: table_name.to_string(),
            values,
            row_idx,
        });
    }

    pub fn commit(&mut self, tx_id: u64) {
        let scn = self.scn.next();
        self.statuses.insert(tx_id, TxStatus::Committed);
        self.commit_scns.insert(tx_id, scn);
        self.redo_buffer.push(RedoRecord::Commit { tx_id, scn });
        if self.trace {
            println!("[TX] COMMIT tx_id={} scn={} -> 触发 LGWR 刷盘", tx_id, scn);
        }
        self.lgwr_flush();
    }

    pub fn rollback(&mut self, tx_id: u64) {
        let scn = self.scn.get();
        self.statuses.insert(tx_id, TxStatus::RolledBack);
        if self.trace {
            println!("[TX] ROLLBACK tx_id={} scn={}", tx_id, scn);
        }
        self.redo_buffer.push(RedoRecord::Rollback { tx_id, scn });
        self.lgwr_flush();
    }

    /// LGWR: 把 redo_buffer 追加写到 redo.log 并清空缓冲区
    fn lgwr_flush(&mut self) {
        if self.redo_buffer.is_empty() {
            return;
        }
        let mut file = self.redo_file.lock().unwrap();
        for rec in &self.redo_buffer {
            let line = serde_json::to_string(rec).unwrap_or_else(|_| "null".to_string());
            writeln!(file, "{}", line).ok();
        }
        file.flush().ok();
        if self.trace {
            println!(
                "[LGWR] 已将 {} 条 redo 记录刷写到 ./redo.log (current scn={})",
                self.redo_buffer.len(),
                self.scn.get()
            );
        }
        self.redo_buffer.clear();
    }

    /// 判断某元组对当前会话是否可见 (Read Committed 隔离)
    /// - 无 tx_id: 可见 (系统数据/恢复数据)
    /// - 当前事务自己写入: 可见
    /// - 已提交且 commit_scn <= cr_scn: 可见
    /// - 其它情况: 不可见
    pub fn is_visible(
        &self,
        row_tx_id: Option<u64>,
        session_tx_id: Option<u64>,
        cr_scn: u64,
    ) -> bool {
        match row_tx_id {
            None => true,
            Some(row_tx) if Some(row_tx) == session_tx_id => true,
            Some(row_tx) => match self.statuses.get(&row_tx) {
                Some(TxStatus::Committed) => {
                    let commit_scn = self.commit_scns.get(&row_tx).copied().unwrap_or(u64::MAX);
                    commit_scn <= cr_scn
                }
                _ => false,
            },
        }
    }
}
