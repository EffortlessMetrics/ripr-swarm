//! Wrapped return-value owner-pin contracts, grouped to share linked code (#12914).

mod wrapped_owner_pins {
    mod claims_payload_eq_owner_pin;
    mod claims_payload_owner_pin;
    mod unwrapped_ok_owner_pin;
}

#[path = "wrapped_owner_pins/inventory_contract.rs"]
mod inventory_contract;
