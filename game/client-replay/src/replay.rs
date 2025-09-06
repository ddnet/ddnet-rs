use std::{
    collections::BTreeMap,
    sync::{Arc, mpsc::sync_channel},
};

use anyhow::anyhow;
use base_io::io::Io;
use client_demo::DemoViewer;
use demo::{
    DemoEvent, DemoEvents, DemoSnapshot,
    recorder::{DemoRecorder, DemoRecorderCreateProps, DemoRecorderCreatePropsBase},
};
use egui::FontDefinitions;
use game_interface::types::game::NonZeroGameTickType;

pub const REPLAY_TIME_SECS: u64 = 30;

#[derive(Debug)]
pub struct Replay {
    pub snapshots: BTreeMap<u64, DemoSnapshot>,
    pub events: BTreeMap<u64, DemoEvents>,

    ticks_per_second: NonZeroGameTickType,

    props: DemoRecorderCreatePropsBase,
    fonts: FontDefinitions,
    io: Io,
    tp: Arc<rayon::ThreadPool>,
}

impl Replay {
    pub fn new(
        io: &Io,
        tp: &Arc<rayon::ThreadPool>,
        fonts: FontDefinitions,
        props: DemoRecorderCreatePropsBase,
        ticks_per_second: NonZeroGameTickType,
    ) -> Self {
        Self {
            snapshots: Default::default(),
            events: Default::default(),
            ticks_per_second,

            io: io.clone(),
            tp: tp.clone(),
            fonts,
            props,
        }
    }

    fn truncate(&mut self, monotonic_tick: u64) {
        self.events = self.events.split_off(
            &monotonic_tick.saturating_sub(self.ticks_per_second.get() * REPLAY_TIME_SECS),
        );
        self.snapshots = self.snapshots.split_off(
            &monotonic_tick.saturating_sub(self.ticks_per_second.get() * REPLAY_TIME_SECS),
        );
    }

    pub fn add_snapshot(&mut self, monotonic_tick: u64, snapshot: Vec<u8>) {
        // if the entry already exist, update if, else create a new
        let entry = self.snapshots.entry(monotonic_tick).or_default();

        *entry = snapshot;

        self.truncate(monotonic_tick);
    }

    pub fn add_event(&mut self, monotonic_tick: u64, event: DemoEvent) {
        // if the entry already exist, update if, else create a new
        let entry = self.events.entry(monotonic_tick).or_default();

        entry.push(event);

        self.truncate(monotonic_tick);
    }

    pub fn to_demo(&mut self) -> anyhow::Result<DemoViewer> {
        let (sender, receiver) = sync_channel(1);

        let mut recorder = DemoRecorder::new(
            DemoRecorderCreateProps {
                base: self.props.clone(),
                io: self.io.clone(),
                in_memory: Some(sender),
            },
            self.ticks_per_second,
            None,
            Some("replay".to_string()),
        );

        for (monotonic_tick, events) in self.events.clone() {
            for event in events {
                recorder.add_event(monotonic_tick, event);
            }
        }
        for (monotonic_tick, snapshot) in self.snapshots.clone() {
            recorder.add_snapshot(monotonic_tick, snapshot);
        }

        drop(recorder);
        receiver
            .recv()
            .map_err(|err| anyhow!(err))
            .and_then(|demo| {
                demo.map(|demo| {
                    DemoViewer::new_from_file(
                        &self.io,
                        &self.tp,
                        "replay".into(),
                        self.fonts.clone(),
                        None,
                        demo,
                    )
                })
            })
    }
}
