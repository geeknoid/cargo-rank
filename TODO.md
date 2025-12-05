# TODO

- CommitActivity needs to consider all commits, not just those of the last 90 days
- Support some means of specific metric be measured
- Count how many uses of unsafe
- History of CSVs
- Add a command-line option to dump crate info as JSON/console/excel/html
- Support generating a JSON report
- Commit activity metric isn't great for monorepos
- Provide a mechanism to clear cached data
- Consider whether Miri is being used in CI for a crate
- Use the humantime crate to allow users to specify cache TTL values in a human-friendly format (e.g., "15m" for 15 minutes, "2h" for 2 hours).
- Make the table headers have usize line counts, and fail to write if # lines > usize::MAX
- Implement retry logic when downloading stuff from the interwebs in general
- Deduplicate items being downloaded in general.
- Verify that the broken doc links are working as appropriate. Maybe getting confused between broken intra-doc links and links to other crates like std
