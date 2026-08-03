On the board, project ACME has an in-progress issue about rate-limiting the
login endpoint. I just finished the first step of Task 1: the burst tests are
in and they fail exactly as the plan intends (6th attempt currently returns
200). Record that progress on the board — and note the finding that the
existing test harness needed a fake-clock helper, which took most of the time.
