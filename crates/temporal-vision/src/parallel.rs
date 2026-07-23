use std::ops::Range;

const MAX_WORKERS: usize = 16;

/// Run a callback over contiguous ranges using at most sixteen scoped workers.
#[allow(dead_code)]
pub(crate) fn for_each_chunk(count: usize, body: impl Fn(Range<usize>) + Sync) {
    let ranges = chunk_ranges(count, None);
    if ranges.len() == 1 {
        body(ranges[0].clone());
        return;
    }

    std::thread::scope(|scope| {
        for range in ranges {
            scope.spawn(|| body(range));
        }
    });
}

/// Map each index in contiguous worker chunks and merge worker-local results in order.
pub(crate) fn map_reduce<T: Send>(
    count: usize,
    init: impl Fn() -> T + Sync,
    fold: impl Fn(&mut T, usize) + Sync,
    merge: impl Fn(T, T) -> T,
) -> T {
    map_reduce_with_workers(count, None, init, fold, merge)
}

#[cfg(test)]
fn map_reduce_for_worker_count<T: Send>(
    count: usize,
    workers: usize,
    init: impl Fn() -> T + Sync,
    fold: impl Fn(&mut T, usize) + Sync,
    merge: impl Fn(T, T) -> T,
) -> T {
    map_reduce_with_workers(count, Some(workers), init, fold, merge)
}

fn map_reduce_with_workers<T: Send>(
    count: usize,
    worker_override: Option<usize>,
    init: impl Fn() -> T + Sync,
    fold: impl Fn(&mut T, usize) + Sync,
    merge: impl Fn(T, T) -> T,
) -> T {
    let ranges = chunk_ranges(count, worker_override);
    if ranges.len() == 1 {
        let mut result = init();
        for index in ranges[0].clone() {
            fold(&mut result, index);
        }
        return result;
    }

    std::thread::scope(|scope| {
        let handles = ranges
            .into_iter()
            .map(|range| {
                scope.spawn(|| {
                    let mut result = init();
                    for index in range {
                        fold(&mut result, index);
                    }
                    result
                })
            })
            .collect::<Vec<_>>();
        let mut handles = handles;
        let mut result = handles
            .remove(0)
            .join()
            .expect("parallel worker must not panic");
        for handle in handles {
            result = merge(
                result,
                handle.join().expect("parallel worker must not panic"),
            );
        }
        result
    })
}

#[allow(clippy::single_range_in_vec_init)]
fn chunk_ranges(count: usize, worker_override: Option<usize>) -> Vec<Range<usize>> {
    if count <= 1 {
        return vec![0..count];
    }
    let available = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(MAX_WORKERS);
    let requested = worker_override
        .or_else(|| {
            std::env::var("PERF_PAIR_WORKERS")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|value| (1..=MAX_WORKERS).contains(value))
        })
        .unwrap_or(available);
    let workers = requested.min(count).max(1);
    if workers == 1 {
        return vec![0..count];
    }
    (0..workers)
        .map(|worker| worker * count / workers..(worker + 1) * count / workers)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::map_reduce_for_worker_count;

    #[test]
    fn fixed_worker_counts_merge_in_identical_order() {
        let expected = map_reduce_for_worker_count(
            120,
            1,
            Vec::new,
            |values, index| values.push(index * 3 + 1),
            |mut left: Vec<usize>, right| {
                left.extend(right);
                left
            },
        );
        for workers in [2, 16] {
            let actual = map_reduce_for_worker_count(
                120,
                workers,
                Vec::new,
                |values, index| values.push(index * 3 + 1),
                |mut left: Vec<usize>, right| {
                    left.extend(right);
                    left
                },
            );
            assert_eq!(actual, expected);
        }
    }
}
