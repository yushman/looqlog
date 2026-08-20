## MODIFIED Requirements

### Requirement: The CLI-supplied path is surfaced as a hint
When the CLI was given a positional path, the page SHALL display that path so the user knows
which file to select, and SHALL state that the file is read by the browser rather than by
the `looqlog` process.

#### Scenario: Hint names the file
- **WHEN** the user runs `looqlog /var/log/app.log` and opens the page
- **THEN** the page prompts to open `/var/log/app.log` and explains that the file is read
  locally by the browser

#### Scenario: No path given
- **WHEN** the user runs `looqlog` in file mode with no path
- **THEN** the page shows a plain "open a log file" prompt with no hint text
