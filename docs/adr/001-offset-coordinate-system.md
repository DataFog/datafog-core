# ADR 001: Offset Coordinate System

## Status

Deferred for production; decided for the POC.

## Context

Rust naturally works with UTF-8 byte offsets. Python exposes Unicode code-point
offsets, while JavaScript uses UTF-16 code-unit offsets. A public scan API must
define which coordinate system its entities return.

## POC Decision

Return zero-based Unicode code-point offsets from the Rust core and every POC
binding. This mirrors the existing Python API and keeps fixture results
identical across targets.

## Production Options

1. Keep Unicode code-point offsets as the shared public contract.
2. Return UTF-8 byte offsets from the Rust core and convert in each binding to
   its host language's native coordinate system.
3. Expose a low-level Rust byte-offset API alongside a binding-facing,
   normalized-offset API.

## Decision Trigger

Choose a production option when the Rust core and its first binding are being
implemented.
