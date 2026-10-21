use super::*;

#[test]
fn keeps_the_channels_asked_for() {
    let dir = std::env::temp_dir().join(format!("pina-test-record-{}", std::process::id()));
    let path = dir.join("Audio Files/take.wav");
    let read = |input: Input| {
        let mut w = Writer::create(&path, 48_000, 4, input).unwrap();
        for frame in 0..3 {
            for ch in 0..4 {
                w.write((frame * 10 + ch) as f32).unwrap();
            }
        }
        assert_eq!(w.finish().unwrap(), 3);
        let mut r = hound::WavReader::open(&path).unwrap();
        let channels = r.spec().channels;
        (
            channels,
            r.samples().map(Result::unwrap).collect::<Vec<f32>>(),
        )
    };
    let stereo = |channel| Input {
        channel,
        stereo: true,
    };
    assert_eq!(read(stereo(0)), (2, vec![0.0, 1.0, 10.0, 11.0, 20.0, 21.0]));
    assert_eq!(read(stereo(2)), (2, vec![2.0, 3.0, 12.0, 13.0, 22.0, 23.0]));
    let mono = Input {
        channel: 1,
        stereo: false,
    };
    assert_eq!(read(mono), (1, vec![1.0, 11.0, 21.0]));
    // Stereo from the last channel has only it.
    assert_eq!(read(stereo(3)), (1, vec![3.0, 13.0, 23.0]));
    std::fs::remove_dir_all(dir).unwrap();
}
