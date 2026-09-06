//! Own effect futures beyond the lifetime of SDK response waiters.
use rmcp::ErrorData;
use std::{
    future::Future,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

#[derive(Clone)]
pub(crate) struct Requests {
    tracker: TaskTracker,
    state: Arc<Mutex<State>>,
}
#[derive(Default)]
struct State {
    deadline: Option<tokio::time::Instant>,
    running: Vec<(CancellationToken, tokio::task::AbortHandle)>,
}
impl Default for Requests {
    fn default() -> Self {
        Self {
            tracker: TaskTracker::new(),
            state: Arc::new(Mutex::new(State::default())),
        }
    }
}
impl Requests {
    pub(crate) async fn run<T: Send + 'static>(
        &self,
        token: CancellationToken,
        future: impl Future<Output = Result<T, ErrorData>> + Send + 'static,
    ) -> Result<T, ErrorData> {
        let task = {
            let mut state = self.state.lock().expect("request ownership lock");
            if state.deadline.is_some() {
                return Err(ErrorData::internal_error(
                    "MCP service is shutting down; request was not dispatched.",
                    None,
                ));
            }
            state.running.retain(|(_, handle)| !handle.is_finished());
            let task = self.tracker.spawn(future);
            state.running.push((token, task.abort_handle()));
            task
        };
        task.await.map_err(|_| {
            ErrorData::internal_error("MCP execution task ended unexpectedly.", None)
        })?
    }
    pub(crate) fn stop(&self, budget: Duration) -> tokio::time::Instant {
        let mut state = self.state.lock().expect("request ownership lock");
        let deadline = *state
            .deadline
            .get_or_insert_with(|| tokio::time::Instant::now() + budget);
        for (token, _) in &state.running {
            token.cancel();
        }
        self.tracker.close();
        deadline
    }
    pub(crate) async fn drained(&self) {
        self.tracker.wait().await;
    }
    pub(crate) fn abort_remaining(&self) {
        for (_, handle) in &self.state.lock().expect("request ownership lock").running {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn dropped_response_waiter_does_not_drop_cleanup() {
        let requests = Requests::default();
        let token = CancellationToken::new();
        let started = Arc::new(tokio::sync::Notify::new());
        let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (worker, started_task, finished_task, ct) = (
            requests.clone(),
            started.clone(),
            finished.clone(),
            token.clone(),
        );
        let waiter = tokio::spawn(async move {
            worker
                .run(ct.clone(), async move {
                    started_task.notify_one();
                    ct.cancelled().await;
                    tokio::task::yield_now().await;
                    finished_task.store(true, std::sync::atomic::Ordering::SeqCst);
                    Ok(())
                })
                .await
        });
        started.notified().await;
        waiter.abort();
        requests.stop(Duration::from_secs(1));
        requests.drained().await;
        assert!(finished.load(std::sync::atomic::Ordering::SeqCst));
        assert!(requests.run(token, async { Ok(()) }).await.is_err());
    }
}
