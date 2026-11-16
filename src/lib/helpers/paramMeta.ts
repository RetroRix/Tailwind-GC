// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Rick Quast

// Readable parameter values (rivals batch, 2026-11-16): wherever Tailwind lists FC parameter
// values, bitmask parameters show the names of their set bits and enum parameters show the
// label next to the number. The tables below cover the commonly-read ArduPilot parameters and
// follow ArduPilot's published parameter definitions; anything not listed shows the plain
// number — never an invented label. Clean-room: written from the public parameter docs, not
// copied from any other ground station's code.

type EnumMeta = { kind: 'enum'; values: Record<number, string> };
type BitmaskMeta = { kind: 'bitmask'; bits: Record<number, string> };
type ParamMeta = EnumMeta | BitmaskMeta;

const COPTER_FLIGHT_MODES: Record<number, string> = {
  0: 'Stabilize', 1: 'Acro', 2: 'Alt Hold', 3: 'Auto', 4: 'Guided', 5: 'Loiter', 6: 'RTL',
  7: 'Circle', 9: 'Land', 11: 'Drift', 13: 'Sport', 14: 'Flip', 15: 'AutoTune', 16: 'PosHold',
  17: 'Brake', 18: 'Throw', 19: 'Avoid ADSB', 20: 'Guided NoGPS', 21: 'Smart RTL',
  22: 'FlowHold', 23: 'Follow', 24: 'ZigZag', 25: 'SystemId', 26: 'Heli Autorotate',
  27: 'Auto RTL',
};

const PLANE_FLIGHT_MODES: Record<number, string> = {
  0: 'Manual', 1: 'Circle', 2: 'Stabilize', 3: 'Training', 4: 'Acro', 5: 'FBW A', 6: 'FBW B',
  7: 'Cruise', 8: 'Autotune', 10: 'Auto', 11: 'RTL', 12: 'Loiter', 13: 'Takeoff',
  14: 'Avoid ADSB', 15: 'Guided', 16: 'Initialising', 17: 'QStabilize', 18: 'QHover',
  19: 'QLoiter', 20: 'QLand', 21: 'QRTL', 22: 'QAutotune', 23: 'QAcro', 24: 'Thermal',
  25: 'Loiter to Alt',
};

const COMMON: Record<string, ParamMeta> = {
  ARMING_CHECK: {
    kind: 'bitmask',
    bits: {
      0: 'All', 1: 'Barometer', 2: 'Compass', 3: 'GPS', 4: 'INS', 5: 'Parameters',
      6: 'RC channels', 7: 'Board voltage', 8: 'Battery', 9: 'Logging', 10: 'Safety switch',
      11: 'GPS config', 12: 'System', 13: 'Mission', 14: 'Rangefinder', 15: 'Camera',
      16: 'Aux auth', 17: 'Vision', 18: 'FFT',
    },
  },
  FENCE_TYPE: {
    kind: 'bitmask',
    bits: { 0: 'Altitude', 1: 'Circle', 2: 'Polygon', 3: 'Circle exclusion' },
  },
  BATT_MONITOR: {
    kind: 'enum',
    values: {
      0: 'Disabled', 3: 'Analog voltage only', 4: 'Analog voltage + current', 5: 'SMBus',
      6: 'Bebop', 7: 'SMBus-Maxell', 8: 'SMBus-SUI', 9: 'DroneCAN', 10: 'ESC telemetry',
      11: 'Synthetic', 12: 'INA2xx', 13: 'Rotoye', 14: 'MPPT', 15: 'Fuel flow',
      16: 'Fuel level (analog)', 17: 'Sum of following', 18: 'INA239', 19: 'EFI',
      20: 'AD7091R5', 21: 'Scripting', 22: 'INA3221', 23: 'Torquemax',
    },
  },
  GPS_TYPE: {
    kind: 'enum',
    values: {
      0: 'None', 1: 'Auto', 2: 'uBlox', 5: 'NMEA', 6: 'SiRF', 7: 'HIL', 8: 'SwiftNav',
      9: 'DroneCAN', 10: 'SBF', 11: 'GSOF', 13: 'ERB', 14: 'MAVLink', 15: 'NOVA',
      16: 'Hemisphere NMEA', 17: 'uBlox yaw', 18: 'NMEA yaw', 19: 'MSP', 20: 'External AHRS',
    },
  },
  COMPASS_LEARN: { kind: 'enum', values: { 0: 'Off', 1: 'Internal', 2: 'Diamond', 3: 'In-flight' } },
  WP_YAW_BEHAVIOR: {
    kind: 'enum',
    values: {
      0: 'Never change yaw', 1: 'Face next waypoint', 2: 'Face next waypoint + RTL',
      3: 'Face along course',
    },
  },
  RTL_ALT_TYPE: { kind: 'enum', values: { 0: 'Relative', 1: 'Terrain' } },
};

const FLTMODE_NAMES = ['FLTMODE1', 'FLTMODE2', 'FLTMODE3', 'FLTMODE4', 'FLTMODE5', 'FLTMODE6'];

/** Human-readable reading of a parameter value, or null when no metadata is known.
 *  `vehicleClass` is Tailwind's ArduPilot class ('copter' | 'plane' | 'quadplane' | …) and only
 *  affects the flight-mode tables. */
export function describeParam(name: string, value: number, vehicleClass?: string | null): string | null {
  if (FLTMODE_NAMES.includes(name)) {
    const table = vehicleClass === 'plane' || vehicleClass === 'quadplane' ? PLANE_FLIGHT_MODES : COPTER_FLIGHT_MODES;
    return table[Math.round(value)] ?? null;
  }
  if (name === 'GPS_TYPE2') {
    const meta = COMMON.GPS_TYPE as EnumMeta;
    return meta.values[Math.round(value)] ?? null;
  }
  const meta = COMMON[name];
  if (!meta) return null;
  if (meta.kind === 'enum') {
    return meta.values[Math.round(value)] ?? null;
  }
  // Bitmask: list the set bits; 0 means nothing is on.
  const v = Math.round(value);
  if (v === 0) return 'None';
  const names: string[] = [];
  for (const [bit, label] of Object.entries(meta.bits)) {
    if (v & (1 << Number(bit))) names.push(label);
  }
  return names.length > 0 ? names.join(', ') : null;
}

/** Format a parameter value the way a pilot expects: integer-typed values without a decimal
 *  point, reals trimmed. `paramType` is the MAVLink MAV_PARAM_TYPE (9/10 = real). */
export function formatParamValue(value: number, paramType?: number): string {
  const isReal = paramType === 9 || paramType === 10;
  if (!isReal && Number.isInteger(value)) return String(value);
  if (Number.isInteger(value)) return String(value);
  return String(Number(value.toFixed(6)));
}
