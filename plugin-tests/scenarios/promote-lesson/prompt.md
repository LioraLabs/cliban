Something worth remembering from today's work in this repo (board project
ACME): `cargo test` here must run with `--test-threads=1` because the fixtures
share a tempdir — parallel runs corrupt it and the failures look like flaky
assertions, not contention. Cost me an hour. Put that where the next session
will actually find it.
