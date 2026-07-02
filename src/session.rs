use std::sync::atomic::{AtomicU64, Ordering};

static SESSION_SEQ: AtomicU64 = AtomicU64::new(1);

/// 单会话私有状态: 不加全局锁, 由 client 连接线程持有
#[derive(Debug, Clone)]
pub struct SessionState {
    pub session_id: u64,
    pub current_tx_id: Option<u64>,
    pub cr_scn: u64,
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            session_id: SESSION_SEQ.fetch_add(1, Ordering::SeqCst),
            current_tx_id: None,
            cr_scn: 0,
        }
    }
}
