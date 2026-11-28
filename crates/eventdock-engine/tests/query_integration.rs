use eventdock_core::{IncomingEvent, IncomingEventRef, LogLevel};
use eventdock_engine::CompactEventStore;
use eventdock_engine::{EventQuery, execute_query};

fn populated_store(
    capacity: usize,
    count: usize,
) -> CompactEventStore {
    let mut store = CompactEventStore::with_limits(capacity, 1024 * 1024);

    for i in 0..count {
        let event =
            IncomingEvent::new("foo", LogLevel::Info, format!("event {i}"));

        store
            .push(IncomingEventRef::from(&event))
            .unwrap();
    }

    store
}

#[test]
fn pages_are_newest_first_and_do_not_overlap() {
    let store = populated_store(10, 5);

    let first = execute_query(
        &store,
        EventQuery {
            before_id: None,
            limit: 2,
            level: None,
        },
    );

    assert_eq!(
        first
            .events
            .iter()
            .map(|e| e.id)
            .collect::<Vec<_>>(),
        [4, 3]
    );

    assert_eq!(first.next_cursor, Some(3));

    let second = execute_query(
        &store,
        EventQuery {
            before_id: first.next_cursor,
            limit: 2,
            level: None,
        },
    );

    assert_eq!(
        second
            .events
            .iter()
            .map(|e| e.id)
            .collect::<Vec<_>>(),
        [2, 1]
    );

    assert_eq!(second.next_cursor, Some(1));

    let third = execute_query(
        &store,
        EventQuery {
            before_id: second.next_cursor,
            limit: 2,
            level: None,
        },
    );

    assert_eq!(
        third
            .events
            .iter()
            .map(|e| e.id)
            .collect::<Vec<_>>(),
        [0]
    );

    assert_eq!(third.next_cursor, None);
}

#[test]
fn eviction_preserves_cursor_semantics() {
    let store = populated_store(2, 5);

    let result = execute_query(
        &store,
        EventQuery {
            before_id: Some(4),
            limit: 10,
            level: None,
        },
    );

    assert_eq!(
        result
            .events
            .iter()
            .map(|e| e.id)
            .collect::<Vec<_>>(),
        [3]
    );

    assert_eq!(result.next_cursor, None);
}

#[test]
fn empty_store_returns_empty_page() {
    let store = CompactEventStore::new();

    let result = execute_query(
        &store,
        EventQuery {
            before_id: None,
            limit: 10,
            level: None,
        },
    );

    assert!(result.events.is_empty());
    assert_eq!(result.next_cursor, None);
}

#[test]
fn incremental_execution_matches_page_results() {
    use eventdock_engine::QueryExecution;

    let store = populated_store(100, 75);
    let query = EventQuery {
        before_id: Some(60),
        limit: 20,
        level: None,
    };
    let expected = execute_query(&store, query);
    let mut execution = QueryExecution::new(query, &store);
    let mut steps = 0;

    while !execution.advance(&store, 3) {
        steps += 1;
        assert!(steps < 100);
    }

    assert!(steps > 1);
    assert_eq!(execution.finish(&store), expected);
}

#[test]
fn incremental_execution_handles_eviction() {
    use eventdock_engine::QueryExecution;

    let mut store = populated_store(5, 5);
    let query = EventQuery {
        before_id: None,
        limit: 5,
        level: None,
    };
    let mut execution = QueryExecution::new(query, &store);
    assert!(!execution.advance(&store, 2));

    for i in 0..5 {
        let event =
            IncomingEvent::new("foo", LogLevel::Info, format!("new {i}"));
        store
            .push(IncomingEventRef::from(&event))
            .unwrap();
    }

    assert!(execution.advance(&store, 2));
    let result = execution.finish(&store);
    assert_eq!(
        result
            .events
            .iter()
            .map(|e| e.id)
            .collect::<Vec<_>>(),
        [4, 3]
    );
    assert_eq!(result.next_cursor, None);
}

#[test]
fn filtered_pages_skip_nonmatching_events() {
    let mut store = CompactEventStore::with_limits(20, 1024 * 1024);
    for i in 0..8 {
        let level = if i % 2 == 0 {
            LogLevel::Error
        } else {
            LogLevel::Info
        };
        let event = IncomingEvent::new("source", level, format!("event {i}"));
        store
            .push(IncomingEventRef::from(&event))
            .unwrap();
    }
    let query = EventQuery {
        before_id: None,
        limit: 2,
        level: Some(LogLevel::Error),
    };
    let first = execute_query(&store, query);
    assert_eq!(
        first
            .events
            .iter()
            .map(|e| e.id)
            .collect::<Vec<_>>(),
        [6, 4]
    );
    assert_eq!(first.next_cursor, Some(4));
    let mut execution = eventdock_engine::QueryExecution::new(query, &store);
    while !execution.advance(&store, 1) {}
    assert_eq!(execution.finish(&store), first);
    let second = execute_query(
        &store,
        EventQuery {
            before_id: first.next_cursor,
            ..query
        },
    );
    assert_eq!(
        second
            .events
            .iter()
            .map(|e| e.id)
            .collect::<Vec<_>>(),
        [2, 0]
    );
    assert_eq!(second.next_cursor, None);
}
