use std::{
    collections::{BTreeMap, VecDeque},
    sync::{mpsc, Arc, Condvar, Mutex},
    thread,
};

trait JobFn: Send {
    fn call(self: Box<Self>);
}

impl<F> JobFn for F
where
    F: FnOnce() + Send + 'static,
{
    fn call(self: Box<Self>) {
        (*self)();
    }
}

type Job = Box<dyn JobFn>;

struct State {
    queue: VecDeque<Job>,
    shutdown: bool,
}

struct Inner {
    state: Mutex<State>,
    available: Condvar,
}

impl Inner {
    fn new() -> Self {
        Self {
            state: Mutex::new(State {
                queue: VecDeque::new(),
                shutdown: false,
            }),
            available: Condvar::new(),
        }
    }

    fn push(&self, job: Job) {
        let mut state = self.state.lock().unwrap();
        if state.shutdown {
            return;
        }
        state.queue.push_back(job);
        self.available.notify_one();
    }

    fn next_job(&self) -> Option<Job> {
        let mut state = self.state.lock().unwrap();
        loop {
            if let Some(job) = state.queue.pop_front() {
                return Some(job);
            }
            if state.shutdown {
                return None;
            }
            state = self.available.wait(state).unwrap();
        }
    }
}

fn worker_loop(inner: Arc<Inner>) {
    while let Some(job) = inner.next_job() {
        job.call();
    }
}

pub struct WorkerPool {
    inner: Arc<Inner>,
}

impl WorkerPool {
    pub fn new(threads: usize) -> Self {
        let thread_count = threads.max(1);
        let inner = Arc::new(Inner::new());
        let mut handles = Vec::with_capacity(thread_count);

        for _ in 0..thread_count {
            let inner_clone = Arc::clone(&inner);
            handles.push(thread::spawn(move || worker_loop(inner_clone)));
        }

        Self { inner }
    }

    fn enqueue_job(&self, job: Job) {
        self.inner.push(job);
    }

    pub fn run_ordered_channel<F, I, R>(&self, tasks: I) -> mpsc::Receiver<R>
    where
        I: IntoIterator<Item = F>,
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let unordered_rx = self.run_indexed_channel(tasks);

        // spawn another thread for organizing the jobs back in-order
        let (ordered_tx, ordered_rx) = mpsc::channel();
        thread::spawn(move || {
            let mut next_expected = 0usize;
            let mut buffer: BTreeMap<usize, R> = BTreeMap::new();

            for (idx, result) in unordered_rx {
                buffer.insert(idx, result);

                while let Some(result) = buffer.remove(&next_expected) {
                    if ordered_tx.send(result).is_err() {
                        return;
                    }
                    next_expected += 1;
                }
            }

            drop(ordered_tx);
        });

        ordered_rx
    }

    pub fn run_channel<F, I, R>(&self, tasks: I) -> impl Iterator<Item = R>
    where
        I: IntoIterator<Item = F>,
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        self.run_indexed_channel(tasks).into_iter().map(|tup| tup.1)
    }

    fn run_indexed_channel<F, I, R>(&self, tasks: I) -> mpsc::Receiver<(usize, R)>
    where
        I: IntoIterator<Item = F>,
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let (unordered_tx, unordered_rx) = mpsc::channel::<(usize, R)>();

        // enqueue all jobs then close the sender
        for (idx, task) in tasks.into_iter().enumerate() {
            let ordered_tx = unordered_tx.clone();
            let job: Job = Box::new(move || {
                let result = task();
                let _ = ordered_tx.send((idx, result));
            });
            self.enqueue_job(job);
        }
        unordered_rx
    }
}

#[cfg(test)]
mod tests {
    use super::WorkerPool;
    use std::{thread, time::Duration};

    #[test]
    fn returns_results_in_submission_order() {
        let pool = WorkerPool::new(2);
        let delays = vec![30u64, 5, 15];
        let receiver = pool.run_ordered_channel(delays.into_iter().map(|delay| {
            move || {
                thread::sleep(Duration::from_millis(delay));
                delay
            }
        }));

        let collected: Vec<u64> = receiver.into_iter().collect();
        assert_eq!(collected, vec![30, 5, 15]);
    }

    #[test]
    fn handles_empty_task_list() {
        let pool = WorkerPool::new(4);
        let receiver = pool.run_ordered_channel(std::iter::empty::<fn() -> u8>());
        let results: Vec<u8> = receiver.into_iter().collect();
        assert!(results.is_empty());
    }

    #[test]
    fn reuses_workers_across_runs() {
        let pool = WorkerPool::new(3);

        for round in 0..3 {
            let receiver = pool.run_ordered_channel((0..5).map(move |n| {
                let value = round * 10 + n;
                move || value
            }));

            let collected: Vec<i32> = receiver.into_iter().collect();
            assert_eq!(
                collected,
                (0..5).map(|n| round * 10 + n).collect::<Vec<_>>()
            );
        }
    }
}
