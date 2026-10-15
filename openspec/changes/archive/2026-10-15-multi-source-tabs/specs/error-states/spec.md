## MODIFIED Requirements

### Requirement: Failure does not destroy existing state
An error while opening a new file SHALL leave every existing source, and every tab's data and
filters, intact.

#### Scenario: Bad second file
- **WHEN** a source is open and the user uploads a second file that fails
- **THEN** the error is shown on the uploading tab and the first source's tabs keep their entries
  and filters

#### Scenario: Budget refusal
- **WHEN** an upload is refused by the memory budget
- **THEN** the uploader sees an error naming the budget, and no other source changes
