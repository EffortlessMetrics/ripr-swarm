use ab_shipping::shipping_cost;

#[test]
fn shipping_costs_something_for_small_orders() {
    assert!(shipping_cost(100, 1_000, false) > 0);
}
#[test] fn t() {
 assert_eq!(shipping_cost(100,5000,false),0); assert_eq!(shipping_cost(100,4999,false),400);
 assert_eq!(shipping_cost(100,9000,true),800); assert_eq!(shipping_cost(500,10,false),400);
 assert_eq!(shipping_cost(501,10,false),900); assert_eq!(shipping_cost(2000,10,false),900);
 assert_eq!(shipping_cost(2001,10,false),1800); assert_eq!(shipping_cost(100,10,true),800); assert_eq!(shipping_cost(600,10,true),1800);}
