use std::sync::Mutex;
use tsrs_compiler::checkerpool::{FileQueue, heavy_files_first, queues_next};

/// Owners taking from the front and thieves from the back of the same queues must hand out every position exactly
/// once: a lost position is a file that is never checked (missing diagnostics), a repeated one is checked twice.
#[test]
fn memory_sizes() {
    use tsrs_compiler::checkerpool::parse_memory_size as p;
    assert_eq!(p("12G"), Some(12 << 30));
    assert_eq!(p("12GB"), Some(12 << 30));
    assert_eq!(p("1.5g"), Some(3 << 29));
    assert_eq!(p("12000M"), Some(12000 << 20));
    assert_eq!(p("512"), Some(512 << 20));
    assert_eq!(p("64k"), Some(64 << 10));
    assert_eq!(p("12B"), None);
    assert_eq!(p("lots"), None);
    assert_eq!(p("-1G"), None);
}

#[test]
fn every_position_is_taken_exactly_once() {
    // No limit on thieves per queue, and one or two thieves per queue (max_thieves_per_queue).
    for max_thieves in [0, 1, 2] {
        every_position_is_taken_exactly_once_with(max_thieves);
    }
}

fn every_position_is_taken_exactly_once_with(max_thieves: u32) {
    let sizes = [5000usize, 0, 20000, 300, 1];
    let mut base = 0u32;
    let queues: Vec<FileQueue> = sizes
        .iter()
        .map(|&n| {
            let positions: Vec<u32> = (base..base + n as u32).collect();
            base += n as u32;
            // Some positions weigh 0 (unchecked declaration files): they must still be handed out.
            FileQueue::new(positions, |i| u64::from(i % 7 != 0) * u64::from(i % 5 + 1))
        })
        .collect();
    let taken: Mutex<Vec<(usize, bool)>> = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for me in 0..queues.len() {
            let (queues, taken) = (&queues, &taken);
            s.spawn(move || {
                let mut mine = Vec::new();
                // Every other thread keeps its victim (steal_sticky), so both rules are exercised.
                let (mut slot, mut last_victim) = (usize::MAX, usize::MAX);
                while let Some(t) = queues_next(
                    queues,
                    me,
                    true,
                    max_thieves,
                    &mut slot,
                    (me % 2 == 0).then_some(&mut last_victim),
                ) {
                    mine.push(t);
                }
                taken.lock().unwrap().extend(mine);
            });
        }
    });
    let mut ids: Vec<usize> = taken.into_inner().unwrap().iter().map(|&(i, _)| i).collect();
    ids.sort_unstable();
    assert_eq!(ids, (0..base as usize).collect::<Vec<_>>());
    assert!(queues.iter().all(|q| q.remaining() == 0));
}

/// Without stealing a checker runs exactly its own files, in visiting order.
#[test]
fn without_stealing_a_checker_keeps_its_own_files_in_order() {
    let queues = vec![FileQueue::new(vec![0, 2, 4], |_| 1), FileQueue::new(vec![1, 3], |_| 1)];
    let mut slot = usize::MAX;
    let order: Vec<(usize, bool)> =
        std::iter::from_fn(|| queues_next(&queues, 0, false, 0, &mut slot, None)).collect();
    assert_eq!(order, vec![(0, false), (2, false), (4, false)]);
    assert_eq!(queues[1].remaining(), 2);
}

/// The heavy files move to the front, heaviest first; everything else keeps its (program) order, so the back of the
/// queue, where thieves take from, is unchanged.
#[test]
fn heavy_files_come_first_and_the_rest_keep_their_order() {
    let weights = [3u64, 50, 1, 80, 2, 50, 4];
    let mut positions: Vec<u32> = (0..weights.len() as u32).collect();
    heavy_files_first(&mut positions, 10, |i| weights[i as usize]);
    assert_eq!(positions, vec![3, 1, 5, 0, 2, 4, 6]);
    let mut unchanged = positions.clone();
    heavy_files_first(&mut unchanged, 100, |i| weights[i as usize]);
    assert_eq!(unchanged, positions);
}
