## Purpose

The `theming` capability covers light and dark appearance: following the system preference by
default, an explicit override that persists, and legibility in both.

## Requirements

### Requirement: Light and dark appearance
The application SHALL provide a light and a dark appearance, following the operating system
preference by default and offering an explicit toggle that overrides it. The chosen override
SHALL persist across reloads on the same machine. Each of the six log levels (TRACE, DEBUG, INFO,
WARN, ERROR, FATAL) SHALL render with its own distinct color in both appearances — no two levels
may share a color, so that "distinguishable" (per the existing legibility scenario below) is a
concrete, checkable guarantee rather than a matter of degree.

#### Scenario: System preference on first load
- **WHEN** the page is opened with no stored preference and the system is set to dark
- **THEN** the dark appearance is used

#### Scenario: Explicit override persists
- **WHEN** the user switches to light and reloads
- **THEN** the light appearance is still in effect

#### Scenario: Both appearances are legible
- **WHEN** either appearance is active
- **THEN** entry levels, gap markers, highlights and the timeline remain distinguishable, with no
  element rendering as invisible against its background

#### Scenario: Every level has its own color
- **WHEN** entries at all six levels (TRACE, DEBUG, INFO, WARN, ERROR, FATAL) are displayed
- **THEN** each level's badge is rendered in a color no other level uses, in both the light and the
  dark appearance

### Requirement: No external resources
The appearance SHALL be implemented without fonts, stylesheets or images loaded from any external
origin, consistent with the CSP and with the privacy guarantee that the application works with the
network disabled.

#### Scenario: Offline rendering
- **WHEN** the network is disabled after page load and the theme is toggled
- **THEN** both appearances render fully
