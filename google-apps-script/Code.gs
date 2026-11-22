/**
 * Google Apps Script Web App webhook for Stopwatch.
 *
 * Writes one row per day to an existing sheet tab:
 *   Column A: Date (YYYY-MM-DD)
 *   Column B: Minutes (rounded to the nearest whole minute)
 * If today's date already exists in Column A, Column B is overwritten.
 *
 * Setup:
 * 1. Open your Google Sheet and go to Extensions -> Apps Script.
 * 2. Select all (Ctrl+A), replace Code.gs with this script, optionally set
 *    TARGET_SHEET_NAME below (or leave "" for the first tab), and save (Ctrl+S).
 * 3. (Optional) Set SHARED_SECRET below (or a Script Property named SHARED_SECRET).
 * 4. Click Deploy -> New deployment (or Manage deployments -> Edit -> New version):
 *      - Type: Web app
 *      - Execute as: Me
 *      - Who has access: Anyone
 * 5. Copy the deployed /exec Web App URL into Stopwatch -> Sheets.
 */

const TARGET_SHEET_NAME = "Azure Training";
const SHARED_SECRET = "";

function doGet() {
  return jsonResponse({
    ok: true,
    message:
      "Stopwatch Google Sheets webhook is active. Send POST requests from Stopwatch.",
  });
}

function doPost(e) {
  let lock = null;
  let hasLock = false;

  try {
    const rawBody = e && e.postData && e.postData.contents;
    if (!rawBody) {
      return jsonResponse({ ok: false, error: "Empty request body." });
    }

    const payload = JSON.parse(rawBody);
    if (!payload || typeof payload !== "object" || Array.isArray(payload)) {
      return jsonResponse({
        ok: false,
        error: "Request body must be a JSON object.",
      });
    }

    const expectedSecret =
      PropertiesService.getScriptProperties().getProperty("SHARED_SECRET") ||
      SHARED_SECRET;
    if (expectedSecret && payload.secret !== expectedSecret) {
      return jsonResponse({ ok: false, error: "Invalid secret token." });
    }

    const spreadsheet = SpreadsheetApp.getActiveSpreadsheet();
    if (!spreadsheet) {
      return jsonResponse({
        ok: false,
        error:
          "No bound spreadsheet found. Open your Google Sheet and create the script via Extensions -> Apps Script.",
      });
    }

    const tz = spreadsheet.getSpreadsheetTimeZone();
    const recordedAt =
      typeof payload.recorded_at_unix_ms === "number" &&
      isFinite(payload.recorded_at_unix_ms)
        ? new Date(payload.recorded_at_unix_ms)
        : new Date();
    const targetDate =
      typeof payload.date === "string" &&
      /^\d{4}-\d{2}-\d{2}$/.test(payload.date.trim())
        ? payload.date.trim()
        : Utilities.formatDate(recordedAt, tz, "yyyy-MM-dd");

    const sheet = resolveTargetSheet(spreadsheet);

    if (payload.action === "ping") {
      return jsonResponse({
        ok: true,
        action: "ping",
        spreadsheet: spreadsheet.getName(),
        sheet: sheet.getName(),
        date: targetDate,
      });
    }

    let minutes;
    if (
      typeof payload.elapsed_ms === "number" &&
      isFinite(payload.elapsed_ms) &&
      payload.elapsed_ms >= 0
    ) {
      minutes = Math.round(payload.elapsed_ms / 60000);
    } else if (
      typeof payload.minutes === "number" &&
      isFinite(payload.minutes) &&
      payload.minutes >= 0
    ) {
      minutes = Math.round(payload.minutes);
    } else {
      return jsonResponse({
        ok: false,
        error: "Missing or invalid elapsed_ms or minutes.",
      });
    }

    lock = LockService.getScriptLock();
    lock.waitLock(15000);
    hasLock = true;

    const lastRow = sheet.getLastRow();
    let targetRow;
    let updated = false;

    if (lastRow === 0) {
      const headerRange = sheet.getRange(1, 1, 1, 2);
      headerRange.setValues([["Date", "Minutes"]]);
      headerRange.setFontWeight("bold");
      sheet.setFrozenRows(1);

      targetRow = 2;
      sheet.getRange(targetRow, 2).setNumberFormat("0");
      sheet.getRange(targetRow, 1, 1, 2).setValues([[targetDate, minutes]]);
    } else {
      const values = sheet.getRange(1, 1, lastRow, 1).getValues();
      const displayValues = sheet.getRange(1, 1, lastRow, 1).getDisplayValues();

      let existingRow = -1;
      let lastNonEmptyColARow = 0;

      for (let i = 0; i < values.length; i++) {
        const val = values[i][0];
        const disp = displayValues[i] ? String(displayValues[i][0] || "") : "";
        if (
          (val !== "" && val !== null && val !== undefined) ||
          disp.trim() !== ""
        ) {
          lastNonEmptyColARow = i + 1;
        }
        if (existingRow === -1 && cellMatchesDate(val, disp, targetDate, tz)) {
          existingRow = i + 1;
        }
      }

      if (existingRow !== -1) {
        targetRow = existingRow;
        updated = true;
        const cell = sheet.getRange(targetRow, 2);
        cell.setNumberFormat("0");
        cell.setValue(minutes);
      } else {
        targetRow = lastNonEmptyColARow + 1;
        sheet.getRange(targetRow, 2).setNumberFormat("0");
        sheet.getRange(targetRow, 1, 1, 2).setValues([[targetDate, minutes]]);
      }
    }

    return jsonResponse({
      ok: true,
      spreadsheet: spreadsheet.getName(),
      sheet: sheet.getName(),
      date: targetDate,
      minutes: minutes,
      row: targetRow,
      updated: updated,
    });
  } catch (err) {
    return jsonResponse({
      ok: false,
      error: err && err.message ? err.message : String(err),
    });
  } finally {
    if (hasLock && lock) {
      lock.releaseLock();
    }
  }
}

function resolveTargetSheet(spreadsheet) {
  const configuredName = (
    PropertiesService.getScriptProperties().getProperty("TARGET_SHEET_NAME") ||
    TARGET_SHEET_NAME
  ).trim();

  if (configuredName) {
    const existing = spreadsheet.getSheetByName(configuredName);
    if (!existing) {
      throw new Error(
        "Sheet tab '" +
          configuredName +
          "' was not found in '" +
          spreadsheet.getName() +
          "'. Check TARGET_SHEET_NAME in Code.gs.",
      );
    }
    return existing;
  }

  const first = spreadsheet.getSheets()[0];
  if (!first) {
    throw new Error("Spreadsheet has no sheet tabs.");
  }
  return first;
}

function cellMatchesDate(value, display, targetDate, tz) {
  if (
    (value instanceof Date ||
      Object.prototype.toString.call(value) === "[object Date]") &&
    !isNaN(value.getTime())
  ) {
    return Utilities.formatDate(value, tz, "yyyy-MM-dd") === targetDate;
  }
  if (typeof value === "string") {
    const trimmed = value.trim();
    if (
      trimmed === targetDate ||
      (trimmed.length >= 10 && trimmed.slice(0, 10) === targetDate)
    ) {
      return true;
    }
  }
  if (typeof display === "string") {
    const trimmed = display.trim();
    if (
      trimmed === targetDate ||
      (trimmed.length >= 10 && trimmed.slice(0, 10) === targetDate)
    ) {
      return true;
    }
  }
  return false;
}

function jsonResponse(obj) {
  return ContentService.createTextOutput(JSON.stringify(obj)).setMimeType(
    ContentService.MimeType.JSON,
  );
}
