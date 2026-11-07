use ab_slug::slugify;

#[test]
fn slugify_produces_output() {
    assert!(!slugify("Hello, World").is_empty());
}
#[test] fn t() {
 assert_eq!(slugify("  Hello,   World! "),"hello-world"); assert_eq!(slugify("ABC"),"abc");
 assert_eq!(slugify("aaaaaaaaaaaaaaaaaaaa"),"aaaaaaaaaaaaaaaaaaaa");
 assert_eq!(slugify("aaaaaaaaaaaaaaaaaaab"),"aaaaaaaaaaaaaaaaaaab"[..20].to_string());
 assert_eq!(slugify("aaaaaaaaaaaaaaaaaaa b"),"aaaaaaaaaaaaaaaaaaa");
 assert_eq!(slugify("a - b"),"a-b");}
