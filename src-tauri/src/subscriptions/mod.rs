pub mod classify;
pub mod http;
pub mod hwid;
pub mod metadata;
pub mod model;
pub mod service;
pub mod store;

#[cfg(test)]
mod tests;

pub use classify::{classify_payload, stable_child_key, ClassifiedChild, ClassifiedPayload};
pub use http::{FetchedPayload, SubscriptionHttpClient};
pub use hwid::HwidStore;
pub use metadata::parse_metadata;
pub use model::{
    ChildProfileSummary, EngineKind, ProviderMetadata, SubscriptionErrorKind, SubscriptionKind,
    SubscriptionLinkRef, SubscriptionLinkSummary, SubscriptionOutbounds, SubscriptionRecord,
    SubscriptionSnapshot, SubscriptionSummary, SubscriptionUserinfo,
};
pub use service::{
    AddSubscriptionInput, LegacySubscriptionInput, RefreshSubscriptionResult, ResolvedChildProfile,
    SubscriptionService,
};
pub use store::SubscriptionStore;
