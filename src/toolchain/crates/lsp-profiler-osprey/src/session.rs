//! One CPU session's tables [PROF-ATTACH-PROTOCOL]. The endpoint streams thread
//! rows and stacks once each and samples that index them; this keeps both
//! tables, symbolizes each stack once, and regroups the samples into one
//! [`SampleBatch`] per sampler tick — the unit lspkit counts shares in.

use std::path::Path;

use lsp_profiler::wire::{StackFrame, ThreadSample};
use lsp_profiler::SampleBatch;
use osprey_profiler::{Image, LiveFrame, LiveSymbols};

use crate::protocol::{CpuBatch, SampleRow, ThreadRow};

/// The `state` of a sample taken while its thread was running.
const STATE_ON_CPU: u8 = 0;

#[derive(Debug)]
pub(crate) struct LiveSession {
    symbols: LiveSymbols,
    threads: Vec<ThreadRow>,
    stacks: Vec<Vec<StackFrame>>,
    include_native: bool,
}

impl LiveSession {
    /// See [`wire_frames`] for what `include_native` keeps.
    pub(crate) fn new(exe: &Path, images: Vec<Image>, include_native: bool) -> Self {
        Self {
            symbols: LiveSymbols::new(exe, images),
            threads: Vec::new(),
            stacks: Vec::new(),
            include_native,
        }
    }

    /// Absorb one batch and return its ticks.
    pub(crate) fn ingest(&mut self, batch: CpuBatch) -> Vec<SampleBatch> {
        if let Some(images) = batch.images {
            self.symbols.replace_images(images);
        }
        self.threads.extend(batch.threads);
        // Without native frames only a sourceless stack keeps another image's
        // frames, so only those are named ([`wire_frames`]).
        let include_native = self.include_native;
        let resolved = if include_native {
            self.symbols.resolve(&batch.stacks)
        } else {
            self.symbols.resolve_source(&batch.stacks)
        };
        self.stacks.extend(
            resolved
                .into_iter()
                .map(|frames| wire_frames(frames, include_native)),
        );
        batch
            .samples
            .chunk_by(|a, b| a.4 == b.4)
            .map(|tick| SampleBatch::new(tick.iter().filter_map(|row| self.sample(row)).collect()))
            .collect()
    }

    /// A row naming a thread or stack the session never received is dropped,
    /// never guessed at.
    fn sample(&self, &(_, thread, stack, state, _): &SampleRow) -> Option<ThreadSample> {
        let row = self.threads.get(usize::try_from(thread).ok()?)?;
        let frames = self.stacks.get(usize::try_from(stack).ok()?)?.clone();
        Some(ThreadSample {
            thread_id: i64::from(thread),
            thread_name: Some(thread_name(row)),
            active: state == STATE_ON_CPU,
            frames,
        })
    }
}

/// The wire frames of one symbolized stack. Without `include_native` a stack
/// keeps only its Osprey source frames, so time in the C runtime or the system
/// is charged to the Osprey function that called it. A stack with no source
/// frame at all — a program built without debug info, or a thread parked in
/// the runtime — keeps every frame instead of vanishing from the profile.
fn wire_frames(frames: Vec<LiveFrame>, include_native: bool) -> Vec<StackFrame> {
    let keep_all = include_native || !frames.iter().any(|frame| frame.user);
    frames
        .into_iter()
        .filter(|frame| keep_all || frame.user)
        .map(|frame| StackFrame {
            name: frame.name,
            file: frame.file,
            line: frame.line.map(i64::from),
            module: frame.module,
        })
        .collect()
}

/// `main`, `fiber 3`, `effect`: a fiber is named by its id.
fn thread_name(row: &ThreadRow) -> String {
    if row.label == "fiber" {
        format!("fiber {}", row.fiber)
    } else {
        row.label.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(include_native: bool) -> LiveSession {
        // No image owns these pcs, so every frame is a hex runtime frame.
        LiveSession::new(Path::new("/bin/app"), Vec::new(), include_native)
    }

    fn batch(samples: Vec<SampleRow>) -> CpuBatch {
        CpuBatch {
            threads: vec![
                ThreadRow {
                    fiber: 0,
                    label: "main".to_owned(),
                },
                ThreadRow {
                    fiber: 3,
                    label: "fiber".to_owned(),
                },
            ],
            stacks: vec![vec![0x10, 0x20], vec![0x30]],
            samples,
            images: None,
        }
    }

    /// (thread id, thread name, active, frame count) of one sample.
    type Shape<'a> = (i64, Option<&'a str>, bool, usize);

    #[test]
    fn samples_regroup_into_one_batch_per_tick_with_named_threads() {
        let ticks = session(true).ingest(batch(vec![
            (1, 0, 0, 0, 5),
            (2, 1, 1, 1, 5),
            (9, 0, 1, 0, 6),
        ]));
        let shape: Vec<Vec<Shape<'_>>> = ticks
            .iter()
            .map(|tick| {
                tick.samples
                    .iter()
                    .map(|s| {
                        (
                            s.thread_id,
                            s.thread_name.as_deref(),
                            s.active,
                            s.frames.len(),
                        )
                    })
                    .collect()
            })
            .collect();
        assert_eq!(
            shape,
            [
                vec![(0, Some("main"), true, 2), (1, Some("fiber 3"), false, 1)],
                vec![(0, Some("main"), true, 1)]
            ]
        );
        assert!(ticks.iter().all(|tick| tick.weight_secs.is_none()));
    }

    #[test]
    fn a_stack_with_no_source_frame_keeps_its_native_frames() {
        let ticks = session(false).ingest(batch(vec![(1, 0, 0, 0, 1)]));
        let frames: Vec<usize> = ticks
            .iter()
            .flat_map(|t| t.samples.iter().map(|s| s.frames.len()))
            .collect();
        assert_eq!(frames, [2]);
    }

    fn live(name: &str, user: bool) -> LiveFrame {
        LiveFrame {
            name: name.to_owned(),
            file: user.then(|| "/src/app.osp".to_owned()),
            line: user.then_some(3),
            user,
            module: Some("app".to_owned()),
        }
    }

    fn names(frames: Vec<StackFrame>) -> Vec<String> {
        frames.into_iter().map(|frame| frame.name).collect()
    }

    #[test]
    fn without_native_frames_source_frames_own_the_time_of_their_callees() {
        let mixed = vec![
            live("malloc", false),
            live("spin", true),
            live("main", true),
        ];
        assert_eq!(names(wire_frames(mixed.clone(), false)), ["spin", "main"]);
        assert_eq!(names(wire_frames(mixed, true)), ["malloc", "spin", "main"]);
        let native_only = vec![live("poll", false), live("fiber_sleep", false)];
        assert_eq!(
            names(wire_frames(native_only, false)),
            ["poll", "fiber_sleep"]
        );
        let spin = wire_frames(vec![live("spin", true)], false);
        let located = spin
            .first()
            .map(|f| (f.file.as_deref(), f.line, f.module.as_deref()));
        assert_eq!(located, Some((Some("/src/app.osp"), Some(3), Some("app"))));
    }

    #[test]
    fn rows_naming_tables_the_session_lacks_are_dropped() {
        let mut live = session(true);
        let ticks = live.ingest(batch(vec![
            (1, 2, 0, 0, 1),
            (1, 0, 7, 0, 1),
            (1, 1, 1, 0, 1),
        ]));
        let kept: Vec<i64> = ticks
            .iter()
            .flat_map(|t| t.samples.iter().map(|s| s.thread_id))
            .collect();
        assert_eq!(kept, [1]);
        // A later batch's rows index everything earlier batches defined.
        let later = CpuBatch {
            threads: Vec::new(),
            stacks: Vec::new(),
            samples: vec![(5, 1, 0, 0, 2)],
            images: Some(Vec::new()),
        };
        let names: Vec<String> = live
            .ingest(later)
            .iter()
            .flat_map(|t| {
                t.samples
                    .iter()
                    .flat_map(|s| s.frames.iter().map(|f| f.name.clone()))
            })
            .collect();
        assert_eq!(names, ["0x10", "0x20"]);
    }
}
