# players

This crate provides abstractions for various media players used by the main crate.
It also includes a test suite for running a testcontainer with a media player, setting it up and running tests against it.

## schemas folder

This is a magic folder containing original schemas and their mangled versions.
The mangled versions are used in codegen of their respective players.

Mangled versions:
- `jellyfin-openapi-stable-models-only.json`
  - Used by `jellyfin` module, had to be mangled because:
    - Routes are too weird for progenitor to handle (e.g. it can't tolerate multiple different response types for the same route)
    - Types that have `additionalProperties: false` have to be set to `true` because while the schema here is frozen in time, the actual API can return more fields than what's in the schema as it evolves, if the flag is set to `false` when a request is made with a field that's not in the schema, serde will fail to deserialize the response.
