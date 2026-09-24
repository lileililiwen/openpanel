# operations-workflows Specification

## Purpose

Operator workflows that cross multiple panels, sites, and
bounded contexts. The application-delivery change adds a new
workflow — register, build, promote, rollback — that
operators run end-to-end through the web UI and the CLI.

## Requirements

## ADDED Requirements

### Requirement: Application Delivery Workflow Is Confirmed End-to-End

The application delivery workflow MUST require a typed
operator confirmation at every mutating step (build, promote,
rollback). The confirmation is captured in the request, recorded
in the audit event, and never exposed in the response.

#### Scenario: Promote without confirmation is refused

- **WHEN** an operator submits a promote request with an empty
  `confirmation` field
- **THEN** the request is refused with a typed
  `ApplicationDeliveryError` and no release state mutation
  occurs.

#### Scenario: Rollback without confirmation is refused

- **WHEN** an operator submits a rollback request with an empty
  `confirmation` field
- **THEN** the request is refused with a typed
  `ApplicationDeliveryError` and no release state mutation
  occurs.
