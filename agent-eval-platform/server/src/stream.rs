//! SSE hub：按 topic（run / batch）组织 broadcast channel（书第 50 章 43.x SSE 模式的多路版）
//! 慢消费者会收到 Lagged，由前端走 REST 全量补偿。

use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::domain::TraceEvent;

#[derive(Hash, PartialEq, Eq, Clone, Debug)]
pub enum Topic {
    Run(Uuid),
    Batch(Uuid),
}

#[derive(Default)]
pub struct StreamHub {
    topics: DashMap<Topic, broadcast::Sender<Arc<TraceEvent>>>,
}

impl StreamHub {
    pub fn publish(&self, topic: Topic, ev: Arc<TraceEvent>) {
        if let Some(tx) = self.topics.get(&topic) {
            let _ = tx.send(ev); // 没有订阅者也无妨
        }
    }

    pub fn subscribe(&self, topic: Topic) -> broadcast::Receiver<Arc<TraceEvent>> {
        self.topics
            .entry(topic)
            .or_insert_with(|| broadcast::channel(1024).0)
            .subscribe()
    }

    /// run 结束后回收 channel，防 DashMap 无限增长
    pub fn retire(&self, topic: &Topic) {
        self.topics.remove(topic);
    }
}
