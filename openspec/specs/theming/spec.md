## Purpose

The `theming` capability covers light and dark appearance: following the system preference by
default, an explicit override that persists, and legibility in both.

## Requirements

### Requirement: Light and dark appearance
The application SHALL provide a light and a dark appearance, following the operating system
preference by default and offering an explicit toggle that overrides it. The chosen override
SHALL persist across reloads on the same machine.

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

### Requirement: No external resources
The appearance SHALL be implemented without fonts, stylesheets or images loaded from any external
origin, consistent with the CSP and with the privacy guarantee that the application works with the
network disabled.

#### Scenario: Offline rendering
- **WHEN** the network is disabled after page load and the theme is toggled
- **THEN** both appearances render fully
