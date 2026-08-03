CI keeps flaking on the acme test suite: `test_ordering` fails roughly one run
in five with a timeout, and a rerun always passes. Nobody is on it right now —
get this onto the board (project ACME) so it isn't lost. It's a bug, and it's
high priority: it's masking real failures.
