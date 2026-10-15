## MODIFIED Requirements

### Requirement: A hash is applied on load
On page load at `/l/<id>` with a hash present, the application SHALL apply the encoded state as its
first query, so the user sees the filtered view rather than the full one followed by a jump.

#### Scenario: Fresh tab reproduces the view
- **WHEN** the URL of a filtered view is opened in a new tab while its source is still loaded
- **THEN** the same source, filters, range and query are active and the view matches

#### Scenario: Format override from the hash
- **WHEN** the hash carries a format override
- **THEN** detection is skipped and the named format is used for that source

### Requirement: The sharing caveat is stated
The UI SHALL make clear that a shared URL carries the search text and field values it encodes,
which are fragments of the log, so a link pasted outside the machine discloses them; and that the
link names a source of a running `looqlog` process, so it does not work on another machine or after
that process exits.

#### Scenario: Sharing affordance carries the warning
- **WHEN** the user copies the URL through an affordance the application provides
- **THEN** both caveats are presented at that moment, not only in the README
