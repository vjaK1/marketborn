import { sendSpeed } from '../ipc';
import { SPEED_LEVELS, useStore } from '../store';
import { SaveMenu } from './SaveMenu';

export function SpeedControls() {
  const speed = useStore((s) => s.speed);
  const setSpeed = useStore((s) => s.setSpeed);

  const pick = (level: number) => {
    setSpeed(level);
    void sendSpeed(level);
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
      <SaveMenu />
    </div>
  );
}
