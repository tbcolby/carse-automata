"""Renode capture sequence for Carse Automata (not yet run).

Run from xous-dev-toolkit:
    python3 scripts/renode_capture.py --app carse --script ../carse-automata/capture.py

Contract: run(ctl, outdir) -> number of failed steps. The app is already launched.
"""
import os


def run(ctl, outdir) -> int:
    failures = 0

    def shot(name):
        nonlocal failures
        if not ctl.try_screenshot(os.path.join(outdir, name)):
            failures += 1

    shot("01_initial.png")
    for _ in range(3):
        ctl.timed_key("Right", after=0.5)
    shot("02_cursor_moved.png")
    ctl.inject_key(" ")                 # tap: touch the cell
    ctl.sleep(1.0)
    shot("03_touched.png")
    for _ in range(6):                  # held space: repeats inside the hold window
        ctl.inject_key(" ")
        ctl.sleep(0.3)
    shot("04_running.png")
    ctl.sleep(1.5)
    shot("05_paused.png")
    ctl.timed_key("BackSpace", after=1.0)  # save and leave
    return failures
