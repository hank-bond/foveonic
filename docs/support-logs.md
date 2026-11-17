---
summary: "Find, review and copy local support logs when reporting a Foveonic problem."
kind: how-to
---

# Share support logs

Foveonic keeps local support logs to help explain startup, loading, editing and export failures. To report a problem, open **Log** in the workspace and choose **Copy run logs** beside the launch where it happened. Choose **Copy logs** to copy all visible retained launches. Paste the report into your issue or message. Each run is one application launch, even if its log spans more than one file.

The copied report includes available application and system details, operation settings, error causes and captured stacks. It can contain photograph and destination filenames, folder paths and camera information. Review it before sharing. Reports do not embed photographs or image pixels. Foveonic does not upload logs automatically.

**Clear** removes the visible history from the Log and future copies. It rotates the current log file while preserving the current launch and its context. Older files can remain on disk within the retention limit. Clear is not a secure deletion tool.

## Find the files

The Log window shows the active directory. If startup fails before the workspace opens, the failure view shows the directory when available. For the standard app, the default locations are:

- macOS: `~/Library/Logs/foveonic`
- Windows: `%LOCALAPPDATA%\foveonic\Logs`

Logs use versioned JSON Lines files. Foveonic keeps up to three closed archive segments plus an active segment for each running instance. A segment normally rotates at 1,000 records or 4 MiB. Active files are protected from other running instances. The visible report notes incomplete history or truncated records when detected.

A review build can use a different application ID and directory. For a controlled diagnostic run, `FOVEONIC_SUPPORT_LOG_DIR` overrides the standard directory. The GUI smoke harness uses its own isolated directory when configured.

Support logs begin after the program starts. Failures before program entry, forced termination and some native or graphics-driver crashes may leave no final error or orderly-exit record. A missing exit record alone does not identify the cause. Stack traces describe where a failure was reported and can need matching release symbols for detailed investigation.
