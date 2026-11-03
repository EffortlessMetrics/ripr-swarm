use owner_pin_control::weight;

#[test]
fn checks_weight_under_the_model_checker() {
    loom::model(|| {
        assert_eq!(weight(4), 12);
    });
}
