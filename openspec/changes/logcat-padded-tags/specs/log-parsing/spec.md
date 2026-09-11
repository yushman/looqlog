## MODIFIED Requirements

### Requirement: logcat records are recognised as a whole shape
The parser SHALL recognise a logcat record as the complete sequence of a `MM-DD hh:mm:ss.mmm`
timestamp, two or three uid/pid/tid columns, a single severity letter from `V D I W E F`, and
a tag terminated by `:`. A column SHALL be all digits, a short lowercase word, or the
`u0_aNN` form. The tag SHALL be permitted to be followed by whitespace before its colon,
since `adb logcat` pads the tag to a fixed column width in `threadtime`, its default format —
`vold    :` and `ActivityManager:` SHALL be recognised alike, and the run of whitespace SHALL
NOT be limited in length. The record SHALL be accepted only when the entire sequence including
the tag's colon matches; a partial match SHALL leave the line to the other shapes rather than
consuming its head. The severity letter SHALL supply the entry's level, since it does not sit
in the position the generic level matcher inspects; the letter `S` (silent) SHALL supply none,
after which the record follows the same fallback path as any other line whose prefix yielded no
level.

#### Scenario: Three-column layout with a numeric uid
- **WHEN** the line is `04-18 19:21:16.151  1000   806   995 D ActivityManager: freezing 2521 com.x`
- **THEN** the entry has a timestamp, level DEBUG, and its message is `freezing 2521 com.x`

#### Scenario: Two-column layout
- **WHEN** the line is `04-21 13:07:51.985   806 29149 W UsbDescriptorParser: Unrecognized len: 58`
- **THEN** the entry has a timestamp and level WARN

#### Scenario: A tag padded to the column width
- **WHEN** the line is `01-01 03:00:01.182   135   135 I vold    : Vold 3.0 (the awakening) firing up`
- **THEN** the entry has a timestamp, level INFO, and its message is
  `Vold 3.0 (the awakening) firing up`

#### Scenario: A padded tag whose message contains a colon
- **WHEN** the line is `01-01 03:00:01.182   135   135 D vold    : Detected support for: ext4 f2fs vfat`
- **THEN** the tag is `vold` and the message is `Detected support for: ext4 f2fs vfat`, the
  scan having stopped at the tag's own colon rather than the message's

#### Scenario: Named and app-uid columns
- **WHEN** a record's first column is `root`, `shell` or `u0_a2` instead of digits
- **THEN** the record is recognised the same way

#### Scenario: A partial match is not consumed
- **WHEN** a line opens with `04-21 13:07:51.985   806 29149 W` but carries no `Tag:`
- **THEN** the line is not treated as logcat and its head is not consumed

#### Scenario: Whitespace after the tag without a colon is not a match
- **WHEN** a line opens with `01-01 03:00:01.182   135   135 I vold    started` and no colon
  terminates the tag
- **THEN** the line is not treated as logcat and its head is not consumed

#### Scenario: Silent severity supplies no level
- **WHEN** a record's severity letter is `S` and its message carries no level word
- **THEN** the entry has no level, because `S` is not mapped onto the level set

#### Scenario: The year is inferred and flagged
- **WHEN** a logcat record is parsed with a caller-supplied reference instant
- **THEN** its timestamp carries the inferred year and the entry is marked year-inferred
