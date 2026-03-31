use factstore::{EventFilter, EventQuery, EventRecord};

use crate::payload_match::payload_subset_matches;

pub(crate) fn matches_query(event_query: &EventQuery, event_record: &EventRecord) -> bool {
    match &event_query.filters {
        None => true,
        Some(filters) if filters.is_empty() => true,
        Some(filters) => filters
            .iter()
            .any(|event_filter| matches_filter(event_filter, event_record)),
    }
}

fn matches_filter(event_filter: &EventFilter, event_record: &EventRecord) -> bool {
    let event_type_matches = match &event_filter.event_types {
        Some(event_types) => event_types
            .iter()
            .any(|event_type| event_type == &event_record.event_type),
        None => true,
    };

    let payload_matches = match &event_filter.payload_predicates {
        Some(payload_predicates) => payload_predicates.iter().any(|payload_predicate| {
            payload_subset_matches(payload_predicate, &event_record.payload)
        }),
        None => true,
    };

    event_type_matches && payload_matches
}
