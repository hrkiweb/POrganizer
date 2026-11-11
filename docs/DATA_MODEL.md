# Organizer data model

The desktop app stores everything in one SQLite file, `organizer.db`, in the
app data folder:

| OS | Location |
|---|---|
| Windows | `%APPDATA%\com.personal.organizer\organizer.db` |
| macOS | `~/Library/Application Support/com.personal.organizer/organizer.db` |
| Linux | `~/.local/share/com.personal.organizer/organizer.db` |

The schema lives in `src-tauri/src/db.rs` (`SCHEMA_V1`, `SCHEMA_V2`). `PRAGMA user_version`
tracks the schema version; every future change is a new numbered migration.

## Storage modes

| Mode | Status | Where data lives | Backups |
|---|---|---|---|
| **Local only** | available | `organizer.db` on this computer | Snapshot copies (`Organizer_YYYY-MM-DD_HHMMSS.db` + `attachments/`) written to a folder you choose, e.g. inside Google Drive, OneDrive, iCloud Drive or Dropbox. Automatic: off / at every start / once a day; keeps the newest N. |
| **Server** | planned | Local `organizer.db` **plus** a sync server (PostgreSQL) | Server-side backups |

Backups are complete, self-contained SQLite files and are safe to sync with
cloud drives. **The live `organizer.db` must not be placed in a cloud-synced
folder**: two devices or a sync client writing at the same time can corrupt it.

## Core tables

### `records`: all organizer data

Each item is one row with a JSON payload. This keeps sync generic (one kind of
object) and lets new fields appear without schema migrations.

| Column | Meaning |
|---|---|
| `id` | `<type>:<key>`, e.g. `day:2026-11-11`, `contact:c1`, `project:p1`, `link:note:n1\|project:p1` |
| `space_id` | sharing unit; `personal` for now |
| `type` | `day`, `week`, `month`, `contact`, `note`, `project`, `task`, `call`, `anniv`, `planner`, `link`, `meta` |
| `data` | JSON payload (NULL once deleted) |
| `date_from`, `date_to` | indexed dates for calendar range queries |
| `parent_id` | e.g. the contact of a call |
| `created_at`, `updated_at`, `updated_by` | timestamps and the device (later: user) that changed it |
| `hlc` | hybrid logical clock `ms-counter-device`, used for conflict resolution |
| `deleted` | tombstone, so deletions sync too |

### `changes`: outbox

One row per local write (`upsert` or `delete`, with its `hlc`). In server
mode, rows with `pushed = 0` are sent to the server and then marked as pushed.

### `spaces`

`personal` always exists. Shared spaces (family, company, …) arrive with the
server, each with a `role` of `owner`, `editor` or `viewer`.

### `blobs`

Attachment files. Files themselves stay in `attachments/` next to the database.
`sha256` makes them content-addressed for upload and deduplication.

### `meta_kv`

Device settings: `device_id`, `backup_folder`, `backup_auto`, `backup_keep`,
`last_backup`; later the server URL and the sign-in state.

## External calendars

Appointments made in the Organizer are rows of `events` in the built-in
calendar `local`; Google and iCloud events will land in the same table with
their own `calendar_id`. (Up to version 0.8, entries were text in the time
slots of `day` records; 0.9 converts them automatically into events one time slot long (one hour on
the 60-minute scale) and keeps their links and categories.)

For local events, `app_data` (added in schema version 2) holds the Organizer's
own JSON for the event: wall-clock start and end (`2026-11-11T10:30`),
categories, repeat rule and excluded dates. The other columns are filled from
it on every save, so the table can be queried and synced like any other
calendar. The app reads events through the same `db_load` / `db_apply`
commands as all other data; ids starting with `event:` go to this table.

### `calendar_accounts`

| Column | Google Calendar | iCloud / CalDAV |
|---|---|---|
| `provider` | `google` | `icloud` (or `caldav` for Nextcloud, Fastmail, …) |
| `username` | Google e-mail | Apple ID |
| `server_url` | – | `https://caldav.icloud.com` |
| `credential_ref` | OAuth refresh token, stored in the **OS keychain** | app-specific password, stored in the **OS keychain** |
| `sync_mode` | `read` (import only) or `two_way` | same |

Secrets are never written to the database: `credential_ref` is only the name
of the keychain entry (Windows Credential Manager / macOS Keychain / Secret
Service on Linux).

### `calendars`

| Column | Google (`calendarList` resource) | CalDAV (calendar collection) |
|---|---|---|
| `remote_id` | `id` | collection href, e.g. `/123456/calendars/home/` |
| `name` | `summary` | `DAV:displayname` |
| `description` | `description` | `CALDAV:calendar-description` |
| `color` | `backgroundColor` | `ICAL:calendar-color` (Apple) |
| `timezone` | `timeZone` | `CALDAV:calendar-timezone` |
| `access_role` | `accessRole` (`owner`, `writer`, `reader`, `freeBusyReader` → `freebusy`) | from `DAV:current-user-privilege-set` |
| `is_primary` | `primary` | – |
| `sync_token` | `nextSyncToken` from `events.list` | `DAV:sync-token` (RFC 6578 sync-collection) |
| `ctag` | – | `CS:getctag` (quick "anything changed?" check) |

The built-in calendar `local` ("Organizer") always exists.

### `events`

| Column | Google (`events` resource) | CalDAV (`VEVENT` in an `.ics` resource) |
|---|---|---|
| `remote_id` | `id` | resource href |
| `ical_uid` | `iCalUID` | `UID` |
| `etag` | `etag` | `DAV:getetag` |
| `remote_updated` | `updated` | `LAST-MODIFIED` / `DTSTAMP` |
| `title` | `summary` | `SUMMARY` |
| `description` | `description` | `DESCRIPTION` |
| `location` | `location` | `LOCATION` |
| `status` | `status` | `STATUS` (lower-cased) |
| `transparency` | `transparency` | `TRANSP` |
| `visibility` | `visibility` | `CLASS` (`PUBLIC` / `PRIVATE` / `CONFIDENTIAL`) |
| `all_day`, `start_date`, `end_date` | `start.date`, `end.date` | `DTSTART;VALUE=DATE`, `DTEND;VALUE=DATE` |
| `start_at`, `end_at` | `start.dateTime`, `end.dateTime` (stored as UTC) | `DTSTART`, `DTEND` / `DURATION` (stored as UTC) |
| `timezone` | `start.timeZone` | `TZID` parameter of `DTSTART` |
| `rrule` | `recurrence[]` line starting `RRULE:` | `RRULE` |
| `exdates`, `rdates` | `recurrence[]` lines `EXDATE:` / `RDATE:` | `EXDATE` / `RDATE` |
| `recurring_event_id` | `recurringEventId` | master event with the same `UID` |
| `original_start` | `originalStartTime` | `RECURRENCE-ID` |
| `organizer_email`, `organizer_name` | `organizer.email`, `organizer.displayName` | `ORGANIZER` (+ `CN`) |
| `color` | `colorId` (resolved through the colors endpoint) | `COLOR` (RFC 7986) |
| `url` | `htmlLink` | `URL` |
| `raw` | full JSON of the event | full `.ics` text |
| `dirty` | 1 = changed in Organizer, push on next sync | same |
| `app_data` | – (Organizer events only) | – |
| `hlc`, `updated_by` | sync clock and device of the last local change | same |

`raw` keeps everything the Organizer does not model (conference links,
attachments, extended properties), so two-way sync never strips data when it
writes an event back.

### `event_attendees`

| Column | Google `attendees[]` | CalDAV `ATTENDEE` |
|---|---|---|
| `email` | `email` | `mailto:` value |
| `name` | `displayName` | `CN` |
| `role` | `optional` → `optional`, else `required` | `ROLE` (`REQ-PARTICIPANT`, `OPT-PARTICIPANT`, `CHAIR`, `NON-PARTICIPANT`) |
| `status` | `responseStatus` (`needsAction` → `needs-action`) | `PARTSTAT` |
| `is_organizer`, `is_self` | `organizer`, `self` | compared with `ORGANIZER` and the account |
| `contact_id` | matched to an Organizer contact by e-mail | same |

### `event_reminders`

| Column | Google `reminders.overrides[]` | CalDAV `VALARM` |
|---|---|---|
| `method` | `popup` / `email` | `ACTION` (`DISPLAY`, `EMAIL`, `AUDIO`) |
| `minutes_before` | `minutes` | negative `TRIGGER` duration, in minutes |

### `calendar_sync_log`

One row per sync run: account, calendar, direction, counts, error. Used for the
"last synced" status and for troubleshooting.

## Sync rules (server mode and calendars)

* **Clock:** every change carries a hybrid logical clock (`hlc`). Ordering
  doesn't depend on a computer's clock being right.
* **Conflicts:** field-level "last change wins" by `hlc`. For long texts
  (note bodies), the losing version is kept as a conflict copy.
* **Deletes:** tombstones (`deleted = 1`); purged after all devices have synced.
* **External calendars:** incremental sync with `sync_token` (Google: on HTTP
  410, a full resync). CalDAV: compare `ctag`, then `sync-collection`; fall
  back to an etag comparison. Writes use `If-Match: <etag>` so remote changes
  are never overwritten blindly.
