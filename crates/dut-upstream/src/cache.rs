//! An in-memory cache that keeps upstream data fresh while shielding upstream
//! from request volume.
//!
//! Each key holds at most one value. Reads are answered from memory while the
//! value is fresh. Once it expires, the first reader refreshes it and every
//! concurrent reader of the same key waits for that one refresh
//! (single-flight). A [`CachePolicy`] decides how long values stay fresh,
//! whether expired values may be served while a background refresh runs
//! (stale-while-revalidate), how long old values may stand in when upstream
//! fails (stale-if-error), and how long to leave upstream alone after a
//! failure.
//!
//! Entries are never evicted, so keys must come from a small, validated set
//! such as the stations of the MTR network, never from free-form input.

use std::{
    collections::HashMap,
    error::Error,
    fmt,
    future::Future,
    hash::Hash,
    sync::{
        Arc, Mutex, MutexGuard, PoisonError, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use jiff::Timestamp;
use thiserror::Error;
use tokio::time::Instant;
use tracing::{Instrument, Span, debug, error, info, info_span, warn};

use dut_core::application::source::{Freshness, Snapshot};
use dut_telemetry::millis;

/// How the adapters' caches treat age and upstream failures.
///
/// Built in `const` items, starting from [`fresh_for`](Self::fresh_for), so
/// inconsistent bounds fail the build:
///
/// ```
/// use std::time::Duration;
///
/// use dut_upstream::CachePolicy;
///
/// const BOARDS: CachePolicy = CachePolicy::fresh_for(Duration::from_secs(10))
///     .hints_between(Duration::from_secs(2), Duration::from_secs(15))
///     .stale_if_error(Duration::from_secs(90))
///     .failure_backoff(Duration::from_secs(5));
/// ```
///
/// ```compile_fail
/// use std::time::Duration;
///
/// use dut_upstream::CachePolicy;
///
/// const BOARDS: CachePolicy = CachePolicy::fresh_for(Duration::from_secs(10))
///     .hints_between(Duration::from_secs(15), Duration::from_secs(2));
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CachePolicy {
    default_ttl: Duration,
    ttl_floor: Duration,
    ttl_ceiling: Duration,
    stale_while_revalidate: Duration,
    stale_if_error: Duration,
    failure_backoff: Duration,
}

impl CachePolicy {
    /// Keeps a value fresh for `default_ttl` when upstream gives no hint.
    ///
    /// Until the other steps say otherwise, upstream hints are taken as
    /// given, readers wait for every refresh, an expired value never stands
    /// in for a failed refresh, and a failure is retried by the next reader.
    pub const fn fresh_for(default_ttl: Duration) -> Self {
        Self {
            default_ttl,
            ttl_floor: Duration::ZERO,
            ttl_ceiling: Duration::MAX,
            stale_while_revalidate: Duration::ZERO,
            stale_if_error: Duration::ZERO,
            failure_backoff: Duration::ZERO,
        }
    }

    /// Bounds upstream's freshness hints, and the default too. The floor
    /// stops a nearly expired upstream response from causing a tight
    /// refetch loop.
    #[must_use]
    pub const fn hints_between(mut self, floor: Duration, ceiling: Duration) -> Self {
        assert!(
            floor.as_nanos() <= ceiling.as_nanos(),
            "a TTL floor must not exceed its ceiling"
        );
        self.ttl_floor = floor;
        self.ttl_ceiling = ceiling;
        self
    }

    /// How long after expiry a value may still be served immediately while a
    /// background refresh runs. Zero makes readers wait for the refresh.
    #[must_use]
    pub const fn stale_while_revalidate(mut self, period: Duration) -> Self {
        self.stale_while_revalidate = period;
        self
    }

    /// How long after expiry a value may stand in when a refresh fails.
    #[must_use]
    pub const fn stale_if_error(mut self, period: Duration) -> Self {
        self.stale_if_error = period;
        self
    }

    /// How long to leave upstream alone after a failed refresh.
    #[must_use]
    pub const fn failure_backoff(mut self, period: Duration) -> Self {
        self.failure_backoff = period;
        self
    }

    /// The freshness period for a new value, given upstream's hint.
    fn ttl(&self, hint: Option<Duration>) -> (Duration, TtlSource) {
        match hint {
            None => (
                self.default_ttl.max(self.ttl_floor).min(self.ttl_ceiling),
                TtlSource::Default,
            ),
            Some(hint) if hint < self.ttl_floor => (self.ttl_floor, TtlSource::Floor),
            Some(hint) if hint > self.ttl_ceiling => (self.ttl_ceiling, TtlSource::Ceiling),
            Some(hint) => (hint, TtlSource::Upstream),
        }
    }
}

/// Where a value's freshness period came from, for logs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TtlSource {
    Upstream,
    Default,
    Floor,
    Ceiling,
}

impl fmt::Display for TtlSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Upstream => "upstream_hint",
            Self::Default => "policy_default",
            Self::Floor => "clamped_to_floor",
            Self::Ceiling => "clamped_to_ceiling",
        })
    }
}

/// A value fetched from upstream, with upstream's own view of its freshness.
#[derive(Debug)]
pub(crate) struct Fetched<V> {
    pub value: V,
    pub ttl_hint: Option<Duration>,
}

#[derive(Debug, Error)]
pub(crate) enum CacheError<E> {
    #[error("the upstream fetch failed and no usable cached value exists")]
    Fetch(#[source] E),

    #[error("upstream failed recently and no usable cached value exists; retrying in {retry_in:?}")]
    BackingOff { retry_in: Duration },
}

pub(crate) struct RefreshingCache<K, V> {
    name: &'static str,
    policy: CachePolicy,
    slots: RwLock<HashMap<K, Arc<Slot<V>>>>,
}

impl<K, V> RefreshingCache<K, V>
where
    K: Clone + Eq + Hash + fmt::Display,
    V: Send + Sync + 'static,
{
    /// Creates an empty cache. `name` identifies it in logs.
    pub(crate) fn new(name: &'static str, policy: CachePolicy) -> Self {
        Self {
            name,
            policy,
            slots: RwLock::new(HashMap::new()),
        }
    }

    /// Returns the value for `key`, calling `fetch` only when the cached value
    /// cannot be served as is.
    ///
    /// `fetch` is `'static` because a stale-while-revalidate refresh runs
    /// after this call has returned.
    pub(crate) async fn get<F, Fut, E>(
        &self,
        key: &K,
        fetch: F,
    ) -> Result<Snapshot<V>, CacheError<E>>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<Fetched<V>, E>> + Send + 'static,
        E: Error + Send + Sync + 'static,
    {
        let slot = self.slot(key);
        let policy = self.policy;
        let span = info_span!("cache", cache = self.name, key = %key);

        async move {
            let now = Instant::now();
            match slot.read(now, &policy) {
                Read::Fresh(snapshot) => {
                    if let Freshness::Fresh { expires_in } = snapshot.freshness() {
                        debug!(expires_in_ms = millis(expires_in), "cache hit");
                    }
                    Ok(snapshot)
                }
                Read::Revalidate { snapshot, overdue } => {
                    slot.revalidate_in_background(overdue, now, policy, fetch);
                    Ok(snapshot)
                }
                Read::Miss => slot.refresh(&policy, fetch).await,
            }
        }
        .instrument(span)
        .await
    }

    fn slot(&self, key: &K) -> Arc<Slot<V>> {
        if let Some(slot) = self
            .slots
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key)
        {
            return Arc::clone(slot);
        }
        let mut slots = self.slots.write().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(
            slots
                .entry(key.clone())
                .or_insert_with(|| Arc::new(Slot::new())),
        )
    }
}

impl<K, V> fmt::Debug for RefreshingCache<K, V> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RefreshingCache")
            .field("name", &self.name)
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

/// Everything cached for one key.
struct Slot<V> {
    state: Mutex<SlotState<V>>,
    /// Held for the duration of a refresh; waiting on it is what coalesces
    /// concurrent refreshes.
    refresh_lock: tokio::sync::Mutex<()>,
    /// Set while a stale-while-revalidate refresh is scheduled.
    revalidating: AtomicBool,
}

struct SlotState<V> {
    entry: Option<Entry<V>>,
    failed_at: Option<Instant>,
}

struct Entry<V> {
    value: Arc<V>,
    fetched_at: Timestamp,
    stored_at: Instant,
    fresh_until: Instant,
}

enum Read<V> {
    Fresh(Snapshot<V>),
    Revalidate {
        snapshot: Snapshot<V>,
        overdue: Duration,
    },
    Miss,
}

impl<V: Send + Sync + 'static> Slot<V> {
    fn new() -> Self {
        Self {
            state: Mutex::new(SlotState {
                entry: None,
                failed_at: None,
            }),
            refresh_lock: tokio::sync::Mutex::new(()),
            revalidating: AtomicBool::new(false),
        }
    }

    fn read(&self, now: Instant, policy: &CachePolicy) -> Read<V> {
        let state = self.state();
        let Some(entry) = &state.entry else {
            return Read::Miss;
        };
        if now < entry.fresh_until {
            return Read::Fresh(entry.fresh_snapshot(now));
        }
        let overdue = now - entry.fresh_until;
        if overdue < policy.stale_while_revalidate {
            Read::Revalidate {
                snapshot: entry.stale_snapshot(),
                overdue,
            }
        } else {
            Read::Miss
        }
    }

    /// Schedules a background refresh unless one is already scheduled or
    /// upstream is in its failure backoff.
    fn revalidate_in_background<F, Fut, E>(
        self: &Arc<Self>,
        overdue: Duration,
        now: Instant,
        policy: CachePolicy,
        fetch: F,
    ) where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<Fetched<V>, E>> + Send + 'static,
        E: Error + Send + Sync + 'static,
    {
        if let Some(retry_in) = self.backoff_remaining(now, &policy) {
            debug!(
                stale_age_ms = millis(overdue),
                retry_in_ms = millis(retry_in),
                "serving stale value; upstream is in failure backoff"
            );
            return;
        }
        if self.revalidating.swap(true, Ordering::AcqRel) {
            debug!(
                stale_age_ms = millis(overdue),
                "serving stale value; background refresh already scheduled"
            );
            return;
        }

        info!(
            stale_age_ms = millis(overdue),
            "serving stale value; refreshing in background"
        );
        let guard = RevalidationGuard(Arc::clone(self));
        tokio::spawn(
            async move {
                // The outcome is logged by `refresh`; readers pick it up from the slot.
                let _ = guard.0.refresh(&policy, fetch).await;
                drop(guard);
            }
            .instrument(Span::current()),
        );
    }

    /// Fetches a new value, unless another reader did so while this one was
    /// waiting, and falls back to a stale value if upstream fails.
    async fn refresh<F, Fut, E>(
        &self,
        policy: &CachePolicy,
        fetch: F,
    ) -> Result<Snapshot<V>, CacheError<E>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Fetched<V>, E>>,
        E: Error + 'static,
    {
        let waiting_since = Instant::now();
        let _refreshing = self.refresh_lock.lock().await;
        let now = Instant::now();

        if let Some(snapshot) = self.fresh_snapshot(now) {
            debug!(
                waited_ms = millis(now - waiting_since),
                "reused the result of a concurrent refresh"
            );
            return Ok(snapshot);
        }

        if let Some(retry_in) = self.backoff_remaining(now, policy) {
            return match self.stale_fallback(now, policy) {
                Some((snapshot, overdue)) => {
                    warn!(
                        retry_in_ms = millis(retry_in),
                        stale_age_ms = millis(overdue),
                        "upstream is in failure backoff; serving stale value"
                    );
                    Ok(snapshot)
                }
                None => {
                    warn!(
                        retry_in_ms = millis(retry_in),
                        "upstream is in failure backoff and no usable stale value exists"
                    );
                    Err(CacheError::BackingOff { retry_in })
                }
            };
        }

        let age_ms = self.stored_age(now).map(millis);
        let reason = if age_ms.is_some() {
            "expired"
        } else {
            "missing"
        };
        info!(reason, age_ms, "refreshing from upstream");

        let fetch_started = Instant::now();
        let result = fetch().await;
        let now = Instant::now();
        let fetch_ms = millis(now - fetch_started);

        match result {
            Ok(fetched) => {
                let (ttl, ttl_source) = policy.ttl(fetched.ttl_hint);
                let snapshot = self.store(fetched.value, ttl, now);
                info!(
                    ttl_ms = millis(ttl),
                    ttl_source = %ttl_source,
                    upstream_ttl_hint_ms = fetched.ttl_hint.map(millis),
                    fetch_ms,
                    "cache refreshed"
                );
                Ok(snapshot)
            }
            Err(fetch_error) => {
                self.state().failed_at = Some(now);
                match self.stale_fallback(now, policy) {
                    Some((snapshot, overdue)) => {
                        warn!(
                            stale_age_ms = millis(overdue),
                            fetch_ms,
                            error = &fetch_error as &dyn Error,
                            "refresh failed; serving stale value"
                        );
                        Ok(snapshot)
                    }
                    None => {
                        error!(
                            fetch_ms,
                            error = &fetch_error as &dyn Error,
                            "refresh failed and no usable stale value exists"
                        );
                        Err(CacheError::Fetch(fetch_error))
                    }
                }
            }
        }
    }

    fn store(&self, value: V, ttl: Duration, now: Instant) -> Snapshot<V> {
        let entry = Entry {
            value: Arc::new(value),
            fetched_at: Timestamp::now(),
            stored_at: now,
            fresh_until: now + ttl,
        };
        let snapshot = entry.fresh_snapshot(now);

        let mut state = self.state();
        state.entry = Some(entry);
        state.failed_at = None;
        snapshot
    }

    fn fresh_snapshot(&self, now: Instant) -> Option<Snapshot<V>> {
        self.state()
            .entry
            .as_ref()
            .filter(|entry| now < entry.fresh_until)
            .map(|entry| entry.fresh_snapshot(now))
    }

    /// A stale value that is still recent enough to stand in for fresh data.
    fn stale_fallback(
        &self,
        now: Instant,
        policy: &CachePolicy,
    ) -> Option<(Snapshot<V>, Duration)> {
        let state = self.state();
        let entry = state.entry.as_ref()?;
        let overdue = now.saturating_duration_since(entry.fresh_until);
        (overdue <= policy.stale_if_error).then(|| (entry.stale_snapshot(), overdue))
    }

    fn backoff_remaining(&self, now: Instant, policy: &CachePolicy) -> Option<Duration> {
        let failed_at = self.state().failed_at?;
        let retry_in = (failed_at + policy.failure_backoff).saturating_duration_since(now);
        (!retry_in.is_zero()).then_some(retry_in)
    }

    fn stored_age(&self, now: Instant) -> Option<Duration> {
        self.state()
            .entry
            .as_ref()
            .map(|entry| now.saturating_duration_since(entry.stored_at))
    }

    fn state(&self) -> MutexGuard<'_, SlotState<V>> {
        // No code panics while holding this lock, so poisoning cannot leave
        // the state half-updated; recover rather than propagate.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl<V> Entry<V> {
    fn fresh_snapshot(&self, now: Instant) -> Snapshot<V> {
        let expires_in = self.fresh_until.saturating_duration_since(now);
        Snapshot::new(
            Arc::clone(&self.value),
            self.fetched_at,
            Freshness::Fresh { expires_in },
        )
    }

    fn stale_snapshot(&self) -> Snapshot<V> {
        Snapshot::new(Arc::clone(&self.value), self.fetched_at, Freshness::Stale)
    }
}

/// Clears a slot's `revalidating` flag when a background refresh ends, even
/// if the task is cancelled.
struct RevalidationGuard<V>(Arc<Slot<V>>);

impl<V> Drop for RevalidationGuard<V> {
    fn drop(&mut self) {
        self.0.revalidating.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::{Ready, ready},
        sync::atomic::AtomicUsize,
    };

    use futures::future::join_all;
    use tokio::time::{advance, sleep};

    use super::*;

    #[derive(Debug, Error)]
    #[error("upstream exploded")]
    struct Boom;

    const TTL: Duration = Duration::from_secs(10);

    fn policy() -> CachePolicy {
        CachePolicy::fresh_for(TTL)
            .hints_between(Duration::from_secs(1), Duration::from_secs(30))
            .stale_if_error(Duration::from_secs(60))
            .failure_backoff(Duration::from_secs(5))
    }

    fn cache(policy: CachePolicy) -> RefreshingCache<&'static str, u32> {
        RefreshingCache::new("test", policy)
    }

    type FetchResult = Result<Fetched<u32>, Boom>;

    fn succeed(
        calls: &Arc<AtomicUsize>,
        value: u32,
    ) -> impl FnOnce() -> Ready<FetchResult> + Send + 'static {
        let calls = Arc::clone(calls);
        move || {
            calls.fetch_add(1, Ordering::SeqCst);
            ready(Ok(Fetched {
                value,
                ttl_hint: None,
            }))
        }
    }

    fn fail(calls: &Arc<AtomicUsize>) -> impl FnOnce() -> Ready<FetchResult> + Send + 'static {
        let calls = Arc::clone(calls);
        move || {
            calls.fetch_add(1, Ordering::SeqCst);
            ready(Err(Boom))
        }
    }

    fn calls(counter: &Arc<AtomicUsize>) -> usize {
        counter.load(Ordering::SeqCst)
    }

    #[test]
    fn ttl_follows_upstream_hints_within_bounds() {
        let policy = policy();
        let seconds = Duration::from_secs;

        assert_eq!(policy.ttl(None), (TTL, TtlSource::Default));
        assert_eq!(
            policy.ttl(Some(seconds(8))),
            (seconds(8), TtlSource::Upstream)
        );
        assert_eq!(
            policy.ttl(Some(Duration::ZERO)),
            (seconds(1), TtlSource::Floor)
        );
        assert_eq!(
            policy.ttl(Some(seconds(99))),
            (seconds(30), TtlSource::Ceiling)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn serves_fresh_values_from_memory() {
        let cache = cache(policy());
        let counter = Arc::new(AtomicUsize::new(0));

        cache
            .get(&"key", succeed(&counter, 1))
            .await
            .expect("first read");
        advance(Duration::from_secs(3)).await;
        let snapshot = cache
            .get(&"key", succeed(&counter, 2))
            .await
            .expect("second read");

        assert_eq!(*snapshot.value(), 1);
        assert_eq!(
            snapshot.freshness(),
            Freshness::Fresh {
                expires_in: Duration::from_secs(7)
            }
        );
        assert_eq!(calls(&counter), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn refetches_after_expiry() {
        let cache = cache(policy());
        let counter = Arc::new(AtomicUsize::new(0));

        cache
            .get(&"key", succeed(&counter, 1))
            .await
            .expect("first read");
        advance(TTL).await;
        let snapshot = cache
            .get(&"key", succeed(&counter, 2))
            .await
            .expect("second read");

        assert_eq!(*snapshot.value(), 2);
        assert_eq!(calls(&counter), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn keeps_keys_independent() {
        let cache = cache(policy());
        let counter = Arc::new(AtomicUsize::new(0));

        cache.get(&"a", succeed(&counter, 1)).await.expect("read a");
        let snapshot = cache.get(&"b", succeed(&counter, 2)).await.expect("read b");

        assert_eq!(*snapshot.value(), 2);
        assert_eq!(calls(&counter), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn coalesces_concurrent_refreshes_into_one_fetch() {
        let cache = cache(policy());
        let counter = Arc::new(AtomicUsize::new(0));

        let readers = (0..100).map(|_| {
            let counter = Arc::clone(&counter);
            cache.get(&"key", move || async move {
                counter.fetch_add(1, Ordering::SeqCst);
                sleep(Duration::from_millis(200)).await;
                Ok::<_, Boom>(Fetched {
                    value: 7,
                    ttl_hint: None,
                })
            })
        });
        let results = join_all(readers).await;

        assert_eq!(calls(&counter), 1);
        assert!(
            results
                .iter()
                .all(|result| matches!(result, Ok(snapshot) if *snapshot.value() == 7))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn serves_a_stale_value_when_refresh_fails() {
        let cache = cache(policy());
        let counter = Arc::new(AtomicUsize::new(0));

        cache
            .get(&"key", succeed(&counter, 1))
            .await
            .expect("first read");
        advance(TTL + Duration::from_secs(30)).await;
        let snapshot = cache
            .get(&"key", fail(&counter))
            .await
            .expect("stale fallback");

        assert_eq!(*snapshot.value(), 1);
        assert_eq!(snapshot.freshness(), Freshness::Stale);
    }

    #[tokio::test(start_paused = true)]
    async fn fails_when_the_stale_value_is_too_old() {
        let cache = cache(policy());
        let counter = Arc::new(AtomicUsize::new(0));

        cache
            .get(&"key", succeed(&counter, 1))
            .await
            .expect("first read");
        advance(TTL + Duration::from_secs(61)).await;
        let result = cache.get(&"key", fail(&counter)).await;

        assert!(matches!(result, Err(CacheError::Fetch(Boom))));
    }

    #[tokio::test(start_paused = true)]
    async fn leaves_upstream_alone_during_failure_backoff() {
        let cache = cache(policy());
        let counter = Arc::new(AtomicUsize::new(0));

        let first = cache.get(&"key", fail(&counter)).await;
        let second = cache.get(&"key", succeed(&counter, 1)).await;
        assert!(matches!(first, Err(CacheError::Fetch(Boom))));
        assert!(matches!(second, Err(CacheError::BackingOff { .. })));
        assert_eq!(calls(&counter), 1);

        advance(Duration::from_secs(5)).await;
        let third = cache
            .get(&"key", succeed(&counter, 1))
            .await
            .expect("retry after backoff");
        assert_eq!(*third.value(), 1);
        assert_eq!(calls(&counter), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn revalidates_in_the_background_within_the_stale_window() {
        let cache = cache(policy().stale_while_revalidate(Duration::from_secs(30)));
        let counter = Arc::new(AtomicUsize::new(0));

        cache
            .get(&"key", succeed(&counter, 1))
            .await
            .expect("first read");
        advance(TTL + Duration::from_secs(5)).await;

        let stale = cache
            .get(&"key", succeed(&counter, 2))
            .await
            .expect("stale read");
        assert_eq!(*stale.value(), 1);
        assert_eq!(stale.freshness(), Freshness::Stale);

        // Let the background refresh run.
        sleep(Duration::from_millis(1)).await;

        let fresh = cache
            .get(&"key", succeed(&counter, 3))
            .await
            .expect("fresh read");
        assert_eq!(*fresh.value(), 2);
        assert!(!fresh.freshness().is_stale());
        assert_eq!(calls(&counter), 2);
    }
}
