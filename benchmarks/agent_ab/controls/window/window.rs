use ab_window::clip_window;

#[test]
fn clip_window_returns_something_in_range() {
    assert!(clip_window(2, 3, 10).is_some());
}
#[test] fn t() {
 assert_eq!(clip_window(2,0,10),None); assert_eq!(clip_window(10,3,10),None); assert_eq!(clip_window(9,3,10),Some((9,10)));
 assert_eq!(clip_window(2,3,10),Some((2,5))); assert_eq!(clip_window(5,5,10),Some((5,10)));
 assert_eq!(clip_window(8,usize::MAX,10),Some((8,10)));}
