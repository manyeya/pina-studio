mod automation;
mod files;

mod regions;
mod takes;
mod tracks;

use super::*;

fn midi(start: u64, length: u64, notes: &[(i64, u8)]) -> Region {
    let notes = notes
        .iter()
        .map(|&(start, pitch)| Note {
            start,
            length: 240,
            pitch,
            velocity: 100,
        })
        .collect();
    Region {
        start,
        length,
        content: Content::Midi {
            notes,
            controls: Vec::new(),
        },
        ..Default::default()
    }
}

fn audio(start: u64, length: u64, offset: u64) -> Region {
    Region {
        start,
        length,
        content: Content::Audio {
            file: "a.wav".into(),
            offset,
            stretch: 1.0,
            flex: Flex::default(),
            follow: false,
            formants: false,
            warps: Vec::new(),
        },
        ..Default::default()
    }
}

fn project(regions: Vec<Region>, instrument: bool) -> Project {
    let track = Track {
        name: "t".into(),
        instrument: instrument.then_some(Instrument::Synth),
        regions,
        ..Default::default()
    };
    Project {
        tracks: vec![track],
        ..Project::new(48_000)
    }
}

fn take(start: i64, pitch: u8) -> Take {
    let notes = vec![Note {
        start: 0,
        length: 240,
        pitch,
        velocity: 100,
    }];
    Take {
        start,
        content: Content::Midi {
            notes,
            controls: Vec::new(),
        },
    }
}

fn folder(p: &Project, at: At) -> (Vec<i64>, Vec<(u64, u64, usize)>) {
    let r = p.region(at);
    let Content::Takes { takes, comp } = &r.content else {
        panic!("not a folder")
    };
    (
        takes.iter().map(|t| t.start).collect(),
        comp_segments(comp, takes.len(), r.length),
    )
}
