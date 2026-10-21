use super::*;

#[test]
fn stream_round_trips() {
    let s = MemoryStream::default();
    let mut buf = *b"hello";
    let mut n = 0;
    unsafe {
        s.write(buf.as_mut_ptr().cast(), 5, &mut n);
        assert_eq!(n, 5);
        s.seek(1, kIBSeekSet as _, std::ptr::null_mut());
        let mut out = [0u8; 8];
        s.read(out.as_mut_ptr().cast(), 8, &mut n);
        assert_eq!(&out[..n as usize], b"ello");
    }
}

#[test]
fn parameter_changes_reuse_queues_per_id() {
    let p = ParameterChanges::with_capacity(2);
    p.add(7, 0, 0.5);
    p.add(7, 10, 0.75);
    p.add(9, 0, 1.0);
    p.add(11, 0, 1.0); // pool full: dropped
    unsafe {
        assert_eq!(p.getParameterCount(), 2);
        let q = vst3::ComRef::from_raw(p.getParameterData(0)).unwrap();
        assert_eq!(q.getParameterId(), 7);
        assert_eq!(q.getPointCount(), 2);
    }
    p.clear();
    unsafe { assert_eq!(p.getParameterCount(), 0) };
}
