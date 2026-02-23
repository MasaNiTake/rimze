# Agents.md

## Overview
This project, "RIMZE," stands for **R**ust **I**mages / **M**anga **Z**ippy **E**gui viewer. It is a fast, lightweight image and manga viewer built with Rust and egui. The primary goal is to minimize unnecessary dependencies and focus heavily on performance and memory efficiency.

## Basic Coding Rules
### 1. DRY (Don't Repeat Yourself) Principle
- Do not write the same logic or processing in multiple places.
- Extract common operations into functions, traits, or macros for reuse.
- Design the code so that a single modification is reflected throughout the application.

### 2. Idiomatic Rust
- **Ownership & Borrowing:** Avoid unnecessary `clone()` operations. Use references (`&` / `&mut`) whenever possible to prevent performance bottlenecks.
- **Error Handling:** Minimize the use of `unwrap()` or `expect()`. Properly propagate and handle errors using `Result` and the `?` operator.
- **Pattern Matching:** Actively utilize `match` and `if let` to handle all possible states comprehensively.

### 3. CI/CD
- Assume that formatting (`cargo fmt`) and static analysis (`cargo clippy`) must pass in CI environments such as GitHub Actions.

### 4. PR (Pull Request) & Commits
- Before applying changes, run `cargo check`, `cargo fmt`, and `cargo clippy` locally to ensure there are no errors or warnings before committing.
- Commit messages must be clear and explicitly describe what was changed.
- **IMPORTANT**: All code comments and git commit messages MUST be written in Japanese (日本語).

### 5. Language Rule
- The code structure, variables, and instructions should be standard programmatic English, but any explanatory comments inside the code, as well as all Git commit messages, absolutely must be written in **Japanese (日本語)**.

## Core Architecture & Technical Requirements

### UI Framework
- Built exclusively with **egui** and `eframe`.
- The layout comprises a left-side file list, top/bottom menus (including a slider), and a central image display panel.

### Asynchronous Operations
- All file system operations and loading must use **Tokio** (`tokio::fs`, `tokio::spawn`, etc.) to prevent blocking the UI thread.
- Use `spawn_blocking` only when interacting with synchronous third-party decoders (e.g., `lopdf`, `image`).

### Supported Formats & Recommended Crates
- **ZIP Files:** Use `async_zip` for non-blocking operations.
- **PDF Files:** Use `lopdf`. *Requires async wrapper considerations.*
- **Images (webp, png, jpg):** Use the `image` crate. *Requires async wrapper considerations.*

### Memory Management & Caching
- Implement an strict LRU (Least Recently Used) caching mechanism for decoded images.
- Cache evictions must aggressively prioritize freeing memory for old images when the user navigates to newer content or changes the sorting order.
- Respect a user-configurable maximum memory footprint.

### Data Flow
- Decoded data must be dispatched to the `egui` UI thread using channels (e.g., `tokio::sync::mpsc::channel`).
- Show the first frame/page immediately, pushing background tasks for subsequent pages.
