//! Facade for `domain::language_availability` (ADR 0025): the declarations
//! live in `ripr-contract`; each item is re-exported at its established path.

pub use ripr_contract::domain::language_availability::{
    AutoDetectionPolicy, AvailabilityState, ConfiguredIntent, LanguageAvailabilityInput,
    LanguageAvailabilityReport, SupportTier, resolve_language_availability,
    resolve_language_availability_all, unavailable_recovery,
};
