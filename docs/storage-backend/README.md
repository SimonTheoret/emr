# erm storage backend: architecture documentation

System under design: the storage backend of `erm`, which persists recorded exchanges and serves them
back for recording and replay.

Status: draft, under review. Nothing in these documents is decided until it is recorded in
[9. Architecture Decisions](09-architecture-decisions.md).

## Sections

1. [Introduction and Goals](01-introduction-and-goals.md)
2. [Architecture Constraints](02-architecture-constraints.md)
3. [Context and Scope](03-context-and-scope.md)
4. [Solution Strategy](04-solution-strategy.md)
5. [Building Block View](05-building-block-view.md)
6. [Runtime View](06-runtime-view.md)
7. [Deployment View](07-deployment-view.md)
8. [Cross-cutting Concepts](08-cross-cutting-concepts.md)
9. [Architecture Decisions](09-architecture-decisions.md)
10. [Quality Requirements](10-quality-requirements.md)
11. [Risks and Technical Debts](11-risks-and-technical-debts.md)
12. [Glossary](12-glossary.md)

## About arc42

This documentation follows the [arc42](https://arc42.org) template (version 9.1, October 2026),
created and maintained by Dr. Peter Hruschka, Dr. Gernot Starke and contributors. Section guidance
is adapted from <https://github.com/arc42/arc42-template>.
