# Theming Guide

`letme` supports custom color palettes to change how its output looks in your terminal.

## Quick Start

1. Create the palettes directory:

   ```sh
   mkdir -p ~/.config/letme/palettes
   ```

2. Create a palette file, e.g. `~/.config/letme/palettes/my-palette.toml`:

   ```toml
   [colors]
   header  = "bold #7C3AED"
   success = "#10B981"
   error   = "bold #EF4444"
   muted   = "#6B7280"
   ```

   You only need the slots you want to change; everything else keeps its
   default style.

3. Activate it in `~/.config/letme/config.toml`:

   ```toml
   palette = "my-palette"
   ```

That's it. The filename (without `.toml`) is the palette name you reference in the config.

## Where Files Go

```
~/.config/letme/
  config.toml                  # main config; set palette = "name" here
  palettes/
    violet-tide.toml           # palette files live here
    monokai.toml
    solarized.toml
```

On Linux this follows XDG (`$XDG_CONFIG_HOME/letme/` or `~/.config/letme/`).
On macOS it's also `~/.config/letme/` (XDG convention).

## Palette File Format

A palette file is a TOML file with a single `[colors]` table. Each key is a **theme slot** and each value is a **style string**.

```toml
[colors]
success = "#10B981"
error   = "bold #EF4444"
header  = "bold fg:#FFFFFF bg:#7C3AED"
```

You only need to include the slots you want to override. Any slot you omit keeps its default style.

## Theme Slots

Every piece of `letme` output is styled through one of these semantic slots:

| Slot       | Used for                                                         | Default                    |
| ---------- | ---------------------------------------------------------------- | -------------------------- |
| `primary`  | Ecosystem names, detector names                                  | magenta                    |
| `accent`   | Arrows (`→`), bullet points (`•`)                                | cyan                       |
| `header`   | Section headings ("Detected ecosystems:", "Available commands:") | bold magenta               |
| `command`  | Command names (`test`, `lint`), the command being executed       | bold                       |
| `info`     | Shell commands being run (`cargo test`, `npm run lint`)          | blue                       |
| `success`  | Passing checks (`✓`), healthy status                             | green                      |
| `error`    | Failing checks (`✗`), error messages, missing binaries           | red                        |
| `warning`  | Warning labels ("Warning:")                                      | yellow                     |
| `muted`    | Secondary text (tier labels, "letme" prefix, skip messages)      | bright black               |
| `hint`     | Suggested fixes in doctor output                                 | italic bright black        |
| `disabled` | Unresolved commands in alias listings                            | strikethrough bright black |

## Style String Syntax

Style values use a simple space-separated mini-DSL. Tokens can appear in any order.

### Colors

- **Hex foreground:** `#RRGGBB` or `fg:#RRGGBB`
- **Hex background:** `bg:#RRGGBB`

```toml
primary = "#7C3AED"              # purple foreground
header  = "fg:#FFFFFF bg:#7C3AED" # white on purple
```

### Modifiers

| Modifier        | Effect             |
| --------------- | ------------------ |
| `bold`          | Bold text          |
| `italic`        | Italic text        |
| `dimmed`        | Dim/faint text     |
| `underline`     | Underlined text    |
| `strikethrough` | Strikethrough text |

### Combining

Mix colors and modifiers freely:

```toml
error   = "bold #EF4444"                  # bold red
header  = "bold underline #7C3AED"        # bold underlined purple
hint    = "italic dimmed fg:#A78BFA"       # italic dim purple
command = "bold fg:#E0E7FF bg:#1E1B4B"     # bold light-on-dark
```

## Example: `violet-tide` Palette

A complete palette with a violet/teal color scheme:

```toml
# ~/.config/letme/palettes/violet-tide.toml

[colors]
primary   = "bold #7C3AED"            # violet
accent    = "#06B6D4"                 # cyan
header    = "bold underline #8B5CF6"  # bright violet
command   = "bold #E0E7FF"            # lavender
info      = "#818CF8"                 # indigo
success   = "#34D399"                 # emerald
error     = "bold #F87171"            # red
warning   = "#FBBF24"                 # amber
muted     = "#6B7280"                 # gray
hint      = "italic #A78BFA"          # soft violet
disabled  = "strikethrough #6B7280"   # struck gray
```

Activate it:

```toml
# ~/.config/letme/config.toml
palette = "violet-tide"
```

## Tips

- **Partial palettes are fine.** You don't need to define all 11 slots. Only override what you want to change.
- **Test with `letme`** (no arguments) to see the info view, which exercises most slots in one screen.
- **Use `letme doctor`** to see `success`, `error`, and `hint` slots in action.
- **Terminal support matters.** Hex colors (`#RRGGBB`) require a terminal with truecolor (24-bit) support. Most modern terminals support this. If yours doesn't, colors will be approximated.
