The `widget caching` milestone on board ACME has the agreed design in its
description but no tickets yet. Slice it into tickets and get them onto the
board under that milestone.

Answers to the open questions, so you don't need to ask: use LRU eviction with a
fixed 10k entry cap, and ship metrics as their own slice after the cache works.
Granularity looks right to me — go ahead and publish once you've decided the
breakdown.
