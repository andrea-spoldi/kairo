## [0.13.2](https://github.com/andrea-spoldi/kairo/compare/v0.13.1...v0.13.2) (2026-05-13)

### Bug Fixes

* **ci:** replace cargo-dist with simple Linux release workflow ([67901d1](https://github.com/andrea-spoldi/kairo/commit/67901d1f28320a157c0f42a869f60a863281af56))

## [0.13.1](https://github.com/andrea-spoldi/kairo/compare/v0.13.0...v0.13.1) (2026-05-11)

### Bug Fixes

* **ci:** run setup-vendor.sh in release workflow before dist ([258cb25](https://github.com/andrea-spoldi/kairo/commit/258cb25ebea3757da52609aff0d4821f9f5b8b8d))

## [0.13.0](https://github.com/andrea-spoldi/kairo/compare/v0.12.1...v0.13.0) (2026-05-08)

### Features

* add full-workspace story mode with mock data ([593c26a](https://github.com/andrea-spoldi/kairo/commit/593c26a7c440ee47e98127c8dd14616b807d8104))
* add story runner for cluster-free UI development ([4a46d8a](https://github.com/andrea-spoldi/kairo/commit/4a46d8afd16ed0cdb173d6b95a0a88baf1df97d5))
* **ci:** add semantic-release pipeline aligned with charon ([f7b5778](https://github.com/andrea-spoldi/kairo/commit/f7b57780a6f6e6d6a9baead7eaa331784ebab72f))

### Bug Fixes

* **ci:** stop release workflow from running on PRs ([39a8183](https://github.com/andrea-spoldi/kairo/commit/39a8183e6c60bb0ddd2cbde2fd27dff8041f429c))
* patch crates/macros/Cargo.toml in setup-vendor.sh ([b502a5f](https://github.com/andrea-spoldi/kairo/commit/b502a5fe4b81a78c767af83d70b70b3a8047b201))
* replace Unicode chars in setup-vendor.sh (bash 3.2 compat) ([6e61244](https://github.com/andrea-spoldi/kairo/commit/6e6124402d6809ed3425585188e1f2636f63eb0d))
* setup-vendor.sh - correct edition to 2024 and fix tab_panel indent ([16273c6](https://github.com/andrea-spoldi/kairo/commit/16273c64fbdf026c536c0e9a0771cfe14fd54bd6))
* setup-vendor.sh - patch workspace dep entries and fix tab_panel pattern ([86c1588](https://github.com/andrea-spoldi/kairo/commit/86c1588156fdf5e1bdb110c6b00db0db5864c82a))

### Code Refactoring

* reduce duplication and add change guards in UI components ([d6e215d](https://github.com/andrea-spoldi/kairo/commit/d6e215dbff2169d1928a3ccbb4f5ffead4cd7632))
* remove phases from CLAUDE.md, and add Karpathy's 4 rules ([e27eaaa](https://github.com/andrea-spoldi/kairo/commit/e27eaaaeab05c78f628646e7fe5ada5c0ad1fb7b))
