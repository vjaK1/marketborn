import { useState } from 'react';
import { saveGame, sendSpeed } from '../ipc';
import { SPEED_LEVELS, useStore } from '../store';

export function SpeedControls() {
  const speed = useStore((s) => s.speed);
  const setSpeed = useStore((s) => s.setSpeed);
  const saveMessage = useStore((s) => s.saveMessage);
  const setSaveMessage = useStore((s) => s.setSaveMessage);
  const [saving, setSaving] = useState(false);

  const pick = (level: number) => {
    setSpeed(level);
    void sendSpeed(level);
  };

  const onSave = async () => {
    setSaving(true);
    setSaveMessage(null);
    try {
      const path = await saveGame();
      setSaveMessage(`Saved → ${path}`);
    } catch (err) {
      setSaveMessage(`Save failed: ${String(err)}`);
    } finally {
      setSaving(false);
      window.setTimeout(() => setSaveMessage(null), 6000);
    }
  };

  return (
    <div className="controls">
      {SPEED_LEVELS.map((s) => (
        <button
          key={s.level}
          className={speed === s.level ? 'active' : ''}
          title={s.title}
          onClick={() => pick(s.level)}
        >
          {s.label}
        </button>
      ))}
      <button className="save" onClick={() => void onSave()} disabled={saving}>
        {saving ? 'Saving…' : 'Save'}
      </button>
      {saveMessage && <span className="toast">{saveMessage}</span>}
    </div>
  );
}
