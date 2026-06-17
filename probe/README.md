# Probe — USB Hub HID Monitor

Monitoraggio realtime degli eventi (tastiera + consumer/media) generati
dall'hub USB Bridgesil (VID `0x05AC`, PID `0x029C`).

Basato su **CGEvent Tap** passivo: intercetta gli eventi a livello sistema
senza interferire con il funzionamento normale dell'hub.
Tutte le interfacce (0, 1, 2) sono osservate simultaneamente.

## Prerequisiti

- macOS (testato su 26.x)
- Python 3.9+
- Permesso **Accessibilità** per il terminale:
  **Impostazioni di Sistema → Privacy e Sicurezza → Accessibilità**
  (non Monitoraggio Input; CGEvent Tap richiede Accessibilità)

## Setup rapido

```bash
./bootstrap.sh
```

## Utilizzo

```bash
source .venv/bin/activate
python monitor.py
```

Oppure:

```bash
./run.sh
```

## Output

```
╔═════════════════════════════════════════════════════════════════╗
║  USB HUB MONITOR —  CGEvent Tap (passive, all interfaces)     ║
║  Events: 14       Ctrl+C to exit                              ║
╠═════════════════════════════════════════════════════════════════╣
║  [Knob & Media Keys]  (Interface 1 + Audio chip)              ║
║    Knob:  ■ Vol+  □ Vol-  □ Mute                              ║
║    Media: □ Play  □ Next  □ Prev                              ║
║                                                               ║
║  [Keyboard Events]  (Interfaces 0 + 2)                        ║
║    ↓  Key 0x01   mods: ⌘                                      ║
║    ↓  Key 0x38   mods: ⌘⇧                                     ║
║    ↑  Key 0x38   mods: ⌘⇧                                     ║
║    ↑  Key 0x01   mods: ⌘                                      ║
║    ...                                                        ║
╚═════════════════════════════════════════════════════════════════╝
```

Premi Ctrl+C per uscire.

## Troubleshooting

### "ERROR: Cannot create CGEvent tap"

Il terminale non ha il permesso Accessibilità. Vai su:
**Impostazioni di Sistema → Privacy e Sicurezza → Accessibilità**

Aggiungi la tua app terminale (Terminal.app, iTerm, VS Code)
e attiva l'interruttore. Riavvia il terminale.

### ModuleNotFoundError

```bash
source .venv/bin/activate
pip install -r requirements.txt
```
