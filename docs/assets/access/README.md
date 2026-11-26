# Access pane screenshots: capture checklist

The Access pane in Settings shows four steps for allowing butterscroll. Each step gets one screenshot, stored here:

| File | Step it illustrates |
|------|---------------------|
| `step-1-add.png` | 1. Click + below the list. |
| `step-2-go-to-folder.png` | 2. Press Cmd-Shift-G, paste the path below, press Return, then click Add. |
| `step-3-override.png` | 3. Choose Override System Protections. |
| `step-4-allow.png` | 4. Acknowledge each item, click Allow System & App Protection Override, then authenticate. |

## Before you capture

- [ ] Use a clean macOS user account or a VM, never your main account.
- [ ] Make sure butterscroll does not have Access yet in that account.

## While you capture

- [ ] Show butterscroll-related UI only; crop other apps out.
- [ ] Capture at 2x, the window only (Cmd-Shift-4, then Space), one PNG per step with the file names above.
- [ ] Keep each PNG 1200 px wide at most.
- [ ] Do the + flow once. Opening + again, picking butterscroll and cancelling can switch Access off again (see [the troubleshooting note](../../troubleshooting/macos-27-3-permission.md)). If that happens, start over from step 1.
- [ ] Note whether `Grant Access…` (which calls `tap::request_access()`) already added butterscroll to the list. If it did, tell the builder: steps 1 and 2 then become "Switch butterscroll on", and their two screenshots are not needed.

## When you are done

- [ ] Commit the PNGs to `docs/assets/access/` and report the `Grant Access…` finding.
