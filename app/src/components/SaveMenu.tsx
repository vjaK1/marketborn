import { useState } from 'react';
import { formatTickLabel } from '../format';
import { listSaves, loadGame, saveGame, type SaveSlot } from '../ipc';
import { useStore } from '../store';

/** Three named player slots; other slots on disk (autosave, quicksave) are
 * load-only. */
const PLAYER_SLOTS = ['slot-1', 'slot-2', 'slot-3'];

export function SaveMenu() {
  const saveMessage = useStore((s) => s.saveMessage);
  const setSaveMessage = useStore((s) => s.setSaveMessage);
  const [open, setOpen] = useState(false);
  const [slots, setSlots] = useState<SaveSlot[]>([]);
  const [busy, setBusy] = useState(false);

  const refresh = () => {
    void listSaves().then(setSlots);
  };

  const toggle = () => {
    if (!open) refresh();
    setOpen(!open);
  };

  const note = (text: string) => {
    setSaveMessage(text);
    window.setTimeout(() => setSaveMessage(null), 6000);
  };

  const onSave = async (slot: string) => {
    setBusy(true);
    try {
      await saveGame(slot);
      note(`Saved ${slot}`);
      refresh();
    } catch (err) {
      note(`Save failed: ${String(err)}`);
    } finally {
      setBusy(false);
    }
  };

  const onLoad = async (slot: string) => {
    setBusy(true);
    try {
      const tick = await loadGame(slot);
      note(`Loaded ${slot} — ${formatTickLabel(tick)}`);
      setOpen(false);
    } catch (err) {
      note(`Load failed: ${String(err)}`);
    } finally {
      setBusy(false);
    }
  };

  const tickOf = (slot: string) =>
    slots.find((s) => s.slot === slot)?.tick ?? null;
  const others = slots.filter((s) => !PLAYER_SLOTS.includes(s.slot));

  return (
    <div className="save-menu">
      <button className="save" onClick={toggle}>
        Saves {open ? '▴' : '▾'}
      </button>
      {saveMessage && <span className="toast">{saveMessage}</span>}
      {open && (
        <div className="save-menu-panel">
          {PLAYER_SLOTS.map((slot) => {
            const tick = tickOf(slot);
            return (
              <div className="save-row" key={slot}>
                <span className="save-name">{slot}</span>
                <span className="save-tick">
                  {tick !== null ? formatTickLabel(tick) : 'empty'}
                </span>
                <button disabled={busy} onClick={() => void onSave(slot)}>
                  Save
                </button>
                <button
                  disabled={busy || tick === null}
                  onClick={() => void onLoad(slot)}
                >
                  Load
                </button>
              </div>
            );
          })}
          {others.map((s) => (
            <div className="save-row" key={s.slot}>
              <span className="save-name">{s.slot}</span>
              <span className="save-tick">
                {s.tick !== null ? formatTickLabel(s.tick) : '—'}
              </span>
              <button
                disabled={busy || s.tick === null}
                onClick={() => void onLoad(s.slot)}
              >
                Load
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
