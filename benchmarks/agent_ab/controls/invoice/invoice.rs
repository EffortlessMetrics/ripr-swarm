use ab_invoice::{invoice_total, Line, Promo};

#[test]
fn invoice_total_is_positive() {
    let lines = [Line { unit_cents: 250, qty: 4 }];
    assert!(invoice_total(&lines, 800, Promo::None) > 0);
}

#[test]
fn promo_does_not_increase_total() {
    let lines = [Line { unit_cents: 250, qty: 4 }];
    assert!(invoice_total(&lines, 800, Promo::PercentOff(10)) <= invoice_total(&lines, 800, Promo::None));
}
fn l(u:u32,q:u32)->Line{Line{unit_cents:u,qty:q}}
#[test] fn ctl(){
 assert_eq!(invoice_total(&[],800,Promo::FlatOff(5)),0);
 assert_eq!(invoice_total(&[l(250,4)],0,Promo::None),1000);
 assert_eq!(invoice_total(&[l(1,1)],5000,Promo::None),2);   // 1.5 -> 2 half up
 assert_eq!(invoice_total(&[l(1,1)],4999,Promo::None),1);
 assert_eq!(invoice_total(&[l(100,5)],0,Promo::BuyNGetOneFree(2)),400);
 assert_eq!(invoice_total(&[l(100,5)],0,Promo::BuyNGetOneFree(0)),500);
 assert_eq!(invoice_total(&[l(100,2)],0,Promo::BuyNGetOneFree(2)),200);
 assert_eq!(invoice_total(&[l(1000,1)],0,Promo::PercentOff(10)),900);
 assert_eq!(invoice_total(&[l(1000,1)],0,Promo::PercentOff(150)),0);
 assert_eq!(invoice_total(&[l(1000,1)],0,Promo::FlatOff(5000)),0);
 assert_eq!(invoice_total(&[l(1000,1)],1000,Promo::FlatOff(500)),550);
 assert_eq!(invoice_total(&[l(1000,1)],0,Promo::None),1000);
}
