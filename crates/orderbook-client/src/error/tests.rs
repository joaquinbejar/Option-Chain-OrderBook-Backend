//! Unit tests for error module.

use super::*;

#[test]
fn test_api_error_display() {
    let error = Error::Api {
        status: 400,
        message: "Bad request".to_string(),
    };

    let display = format!("{}", error);
    assert!(display.contains("400"));
    assert!(display.contains("Bad request"));
}

#[test]
fn test_not_found_error_display() {
    let error = Error::NotFound("Resource not found".to_string());

    let display = format!("{}", error);
    assert!(display.contains("Not found"));
    assert!(display.contains("Resource not found"));
}

#[test]
fn test_invalid_request_error_display() {
    let error = Error::InvalidRequest("Missing required field".to_string());

    let display = format!("{}", error);
    assert!(display.contains("Invalid request"));
    assert!(display.contains("Missing required field"));
}

#[test]
fn test_connection_closed_error_display() {
    let error = Error::ConnectionClosed;

    let display = format!("{}", error);
    assert!(display.contains("Connection closed"));
}

#[test]
fn test_error_debug() {
    let error = Error::Api {
        status: 500,
        message: "Internal server error".to_string(),
    };

    let debug = format!("{:?}", error);
    assert!(debug.contains("Api"));
    assert!(debug.contains("500"));
}
