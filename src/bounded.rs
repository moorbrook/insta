//! Bounded ownership for independently spawned asynchronous work.

use std::future::Future;
use std::num::NonZeroUsize;
use tokio::task::{JoinError, JoinSet};

/// Run at most `limit` tasks at once and observe every task result.
///
/// Capacity is acquired before spawning because this function never places more
/// than `limit` tasks in the [`JoinSet`]. Dropping this future drops the set,
/// which aborts all still-owned tasks.
pub async fn for_each_bounded<I, Item, MakeFuture, Fut, Output, OnComplete>(
    items: I,
    limit: NonZeroUsize,
    mut make_future: MakeFuture,
    mut on_complete: OnComplete,
) where
    I: IntoIterator<Item = Item>,
    Item: Send + 'static,
    MakeFuture: FnMut(Item) -> Fut,
    Fut: Future<Output = Output> + Send + 'static,
    Output: Send + 'static,
    OnComplete: FnMut(Result<Output, JoinError>),
{
    let mut items = items.into_iter();
    let mut tasks = JoinSet::new();
    let mut exhausted = false;

    loop {
        while !exhausted && tasks.len() < limit.get() {
            if let Some(item) = items.next() {
                tasks.spawn(make_future(item));
            } else {
                exhausted = true;
            }
        }

        let Some(result) = tasks.join_next().await else {
            break;
        };
        on_complete(result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::sync::Barrier;

    #[tokio::test(flavor = "current_thread")]
    async fn admission_never_exceeds_limit() {
        let limit = NonZeroUsize::new(3).unwrap();
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(limit.get()));
        let mut completed = 0;

        for_each_bounded(
            0..12,
            limit,
            {
                let active = Arc::clone(&active);
                let peak = Arc::clone(&peak);
                let barrier = Arc::clone(&barrier);
                move |_| {
                    let active = Arc::clone(&active);
                    let peak = Arc::clone(&peak);
                    let barrier = Arc::clone(&barrier);
                    async move {
                        let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(now, Ordering::SeqCst);
                        barrier.wait().await;
                        active.fetch_sub(1, Ordering::SeqCst);
                    }
                }
            },
            |result| {
                result.unwrap();
                completed += 1;
            },
        )
        .await;

        assert_eq!(completed, 12);
        assert_eq!(active.load(Ordering::SeqCst), 0);
        assert_eq!(peak.load(Ordering::SeqCst), limit.get());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn task_panics_are_observed_without_stopping_the_drain() {
        let mut completed = 0;
        let mut panics = 0;

        for_each_bounded(
            0..4,
            NonZeroUsize::new(2).unwrap(),
            |item| async move {
                assert_ne!(item, 2, "representative task failure");
            },
            |result| {
                completed += 1;
                if result.is_err() {
                    panics += 1;
                }
            },
        )
        .await;

        assert_eq!(completed, 4);
        assert_eq!(panics, 1);
    }
}
