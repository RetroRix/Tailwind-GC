# Parameters

The **Parameters** tab (shown while connected over MAVLink) works with the flight controller's
full parameter list: download it, read it comfortably, save it to a file, and load a file back.
It is deliberately a transfer and reading tool, not a parameter editor — values change only by
loading a file, with the changes shown first.

## Downloading

Press **Download from FC**. Tailwind asks the flight controller for its whole list (a few
hundred to over a thousand parameters, depending on the firmware) and shows them sorted by
name. If some parameters do not answer, the list is marked incomplete and says how many of the
expected total arrived — download again before relying on it.

## Reading values

Next to many values you will find a plain-language reading:

- **Bitmask parameters** (for example `ARMING_CHECK`, `FENCE_TYPE`) list the names of the
  options that are switched on; `None` means none are.
- **Enum parameters** (for example the `FLTMODE1`–`FLTMODE6` flight modes, `BATT_MONITOR`,
  `GPS_TYPE`) show the label for the current number.

Parameters without a known reading show just the number — Tailwind never guesses a label.

## QGroundControl `.params` files

**Save .params** writes the downloaded list in the parameter-file format QGroundControl uses,
so the same file opens in QGroundControl or Mission Planner, and files written by those tools
load here.

**Load .params…** reads a file and compares it against the list you downloaded:

- Every value that differs is listed (old → new) and counted **before** anything is written.
  Nothing is sent until you press **Write to FC**.
- Parameters the connected flight controller does not have are skipped and counted, never
  written blind.
- Writing is **refused while armed** — disarm first.
- After a write, re-download if you want the FC's own confirmation of the new values; the
  panel updates its snapshot with what it wrote, but only the FC knows whether a value was
  accepted and stored.

!!! warning "Parameters are powerful"
    A `.params` file from another vehicle — or another firmware version — can carry values
    that are wrong or unsafe for yours. Review the difference list line by line before
    writing, and keep a saved copy of your own working set first.
