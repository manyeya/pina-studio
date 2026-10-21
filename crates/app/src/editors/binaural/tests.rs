use super::*;

#[test]
fn a_spot_in_the_view_finds_the_point_that_shows_there() {
    let camera = Camera::new(ORBIT);
    let p = [1.2, 0.4, -0.7];
    let (x, y, _) = camera.project(p).unwrap();
    let back = camera.at_height((x, y), 0.4).unwrap();
    assert!(
        back.iter().zip(p).all(|(a, b)| (a - b).abs() < 1e-3),
        "{back:?}"
    );
    // Seen from behind, the head's right is the view's right and front is further up.
    let (rx, ..) = camera.project([1.0, 0.0, 0.0]).unwrap();
    let (_, fy, _) = camera.project([0.0, 0.0, 1.0]).unwrap();
    let (cx, cy, _) = camera.project([0.0, 0.0, 0.0]).unwrap();
    assert!(rx > cx && fy < cy);
    assert_eq!(azimuth_label(-0.25), "L45°");
}
