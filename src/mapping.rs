pub const RAW_MAPPING: &[(&str, &str)] = &[
    ("Top Level Timeline Background", "Operator1"), //
    ("Dark Timeline Background", "SurfaceArea"), // Timeline BG (no tracks area)
    ("Light Timeline Background", "SurfaceBackground"), // Timeline BG (tracks area)
    ("Irrelevant Timeline Background", "SurfaceArea"), // Top line in event editor indicating non-related clip area (out of bounds)
    // ("Top Level Timeline Header Background", "AbletonColor"), // ---
    ("Dark Timeline Header Background", "SurfaceAreaFocus"), // Top timeline header area (ruler for time and bar)
    ("Light Timeline Header Background", "SurfaceAreaFocus"), // Bottom timeline header area (loop range, cue markers and clip start end markers) and named notes area in piano roll
    // ("Irrelevant Timeline Header Background", "AbletonColor"), // ---
    // ----------------------------------------
    // ("Default text", "AbletonColor"), // ---
    ("Window background", "Desktop"), // Outer void, main Bitwig window body color, preferences BG
    ("Panel body", "Desktop"), // Top panel, non-selected track headers, scene headers, device BG
    ("Panel stroke", "SurfaceHighlight"), // Non-selected GUI section, preferences window contour
    ("Active Panel stroke", "ChosenDefault"), // Selected GUI section
    ("Hole (dark)", "SurfaceBackground"), // Browser void, device area void, track headers and clip launcher void 
    ("Hole (medium)", "TransportOffBackground"), // Side-browser heading, empty clip launcher cells, clip launcher/arranger switch selected button, device mini-stack BG, track inspector tab content bg, popup browser panels, device knob sections, VU meter BG, 
    ("Hole (light)", "TransportOffBackground"), // Secondary areas - scrollbar BG, modulator slots, device control areas
    // ("Shadow", "AbletonColor"), // Used in linear gradient for scrollable area fadeouts
    // ("Selected Dashboard Tree", "AbletonColor"), // Selected entry background in dashboard (package man, recent projects, etc)
    ("Selected Panel body", "SurfaceHighlight"), // Selected track header
    // ("Selected Panel stroke", "AbletonColor"), // ---
    // ("Selected Panel stroke (standby)", "AbletonColor"), // ---
    ("Popup insert", "RetroDisplayForeground"), // Popup browser contour + contour of device it replaces
    ("Button stroke", "SurfaceBackground"), // Button contour
    // ("Light button stroke", "AbletonColor"), // ---
    // ("Rubber button stroke", "AbletonColor"), // ---
    ("Button background", "ControlTextBack"), // Literally, button background (non-highlighted)
    // ("Pressed button background", "AbletonColor"), // When non-highlighted button pressed by cursor
    // ("Checkbox background", "AbletonColor"), // Non-highlighted checkbox background (found in prefs)
    // ("Button in tree background", "AbletonColor"), // ---
    // ("View button background", "AbletonColor"), // Non-highlighted button in arranger statusbar (grid snap, track header view options)
    ("Pressed view button background", "SurfaceBackground"), // Highlighted button in arranger statusbar (track header view options, etc)
    // ("Inverted Selected Borderless Button background", "AbletonColor"), // ---
    // ("Selected borderless button background", "AbletonColor"), // Controller prefs options buttons
    // ("Pressed borderless button background", "AbletonColor"), // Controller prefs options buttons
    // ("Rubber highlight button stroke", "AbletonColor"), // ---
    // ("Rubber Button Emboss Highlight", "AbletonColor"), // ---
    ("Selection", "RangeEditField"), // Selected text BG
    ("Standby selection", "RangeEditField"), // Selected text BG (inactive window)
    ("On", "ChosenDefault"), // Main accent color (not only buttons, but also track faders and some accent labels)
    // ("On (subtle)", "AbletonColor"), // ---
    // ("On (subtler)", "AbletonColor"), // ---
    ("Pressed On", "ChosenPlay"), // Pressed highglighted button
    // ("Implicit On (subtle)", "AbletonColor"), // ---
    ("Hitech on", "ChosenDefault"), // Top display text (BPM, bar), some selected values in prefs, badges background, active browser icon in device, selected browser filter FG, play marker
    // ("Hitech background", "AbletonColor"), // ---
    // ("Mapping", "AbletonColor"), // Various controller mapping highlights
    // ("Notification Background", "AbletonColor"), // Notification BG
    // ("Notification Normal", "AbletonColor"), // Notification FG
    // ("Notification Error", "AbletonColor"), // Notification FG (Error)
    // ("Warning", "AbletonColor"), // Probably, some warning highlight (not sure)
    // ("Automation Color", "AbletonColor"), // ---
    // ("User Automation Override Color", "AbletonColor"), // Small dot at control and icon at top display
    // ("Modulation Mapping Color", "AbletonColor"), // Overlay for modulatable controls, positive modulation amount, modulator arrow
    // ("Modulation Mapping Background Color", "AbletonColor"), // Background higlight in inspector modulations list when hovering over modulator arrow
    // ("Modulation Mapping Color (subtractive)", "AbletonColor"), // Literally
    // ("Modulation Mapping Color (polyphonic)", "AbletonColor"), // Literally
    // ("Modulation Mapping Background (polyphonic)", "AbletonColor"), // Literally
    // ("Menu background", "AbletonColor"), // Popup menu background
    // ("Menu stroke", "AbletonColor"), // Popup menu contour
    // ("Menu text", "AbletonColor"), // Popup menu FG
    // ("Menu Icon", "AbletonColor"), // Popup menu icons
    // ("Menu description text", "AbletonColor"), // ---
    // ("Menu separator", "AbletonColor"), // Popup menu separators
    ("Field background", "ControlTextBack"), // Light input BG color (like in Amp device)
    ("Scrollbar", "SurfaceHighlight"), // Scrollbar thumb
    ("Dark Text", "ControlForeground"), // Light input text color (like in Amp device)
    ("Subtle Dark Text", "Operator2"), //
    ("Subtle Light Text", "ChosenDefault"), // Non-selected text or very-secondary
    ("Subtler Light Text", "SurfaceHighlight"), // Browser header separator line
    ("Medium Light Text", "ControlOffForeground"), // Secondary text
    ("Light Text", "ControlForeground"), // Most of the text
    // ----------------------------------------
    ("Knob Body Lighter", "SurfaceHighlight"), // Lighter knob body
    ("Knob Body Lightest", "ChosenDefault"), // Lightest knob body
    ("Knob Line Dark", "SurfaceBackground"), // Knob line on light knobs
    ("Knob Value Background", "Poti"), // Knob stroke BG
    ("Knob Value Color", "RangeDefault"), // Knob stroke value (most of the knobs)
    // ----------------------------------------
    ("Meter Hitech", "ChosenDefault"), // CPU load meter
    ("Meter Hitech Background", "ControlTextBack"), // CPU load meter BG
    // ----------------------------------------
    ("Display Background", "RetroDisplayBackground"), // Some dark text inputs, slider holes, sampler waveform display BG, top display BG, VU meters BG for tracks, many device displays (Amp, etc), mixer channel text notes area BG, clip launcher clips separators, shortcuts prefs BG and other prefs fields BGs
    ("Display Waveform", "RetroDisplayForeground"), // Waveform in sampler
    // ----------------------------------------
    ("Popup overlay background color", "SpectrumGridLines"), // Backdrop overlay for popups
    // ----------------------------------------
    ("Dark tree background (selected)", "SelectionBackground"), // Browser selected item BG
    ("Dark tree background (standby selected)", "StandbySelectionBackground"), // Browser selected item BG (non-focused pane)
    ("Dark tree text", "ControlForeground"), // Browser item FG
    ("Dark tree text (selected)", "SelectionForeground"), // Browser selected item FG
    // ----------------------------------------
    ("Device Header", "Desktop"), // Device headers
    ("Device Header (selected)", "SurfaceHighlight"), // Device headers (selected)
    ("The Grid (background)", "SurfaceArea"), // The Grid BG
    ("The Grid (stroke)", "Desktop"), // The Grid BG lines
];
