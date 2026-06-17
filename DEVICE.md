# USB Hub HID Device — Technical Reference

> **Prodotto**: Hagibis UC-1102AG — USB-C Hub with Knob Shortcut Buttons
> **URL**: https://hagibis.com/products/usb-c-hub-with-knob-shortcut-buttons-376
> **Revision**: 15 Giugno 2026
> **Piattaforma di verifica**: macOS Tahoe (26.x) su Apple Silicon (T8132)
> **Metodi**: `system_profiler SPUSBHostDataType`, `ioreg`, `hidutil list`,
> `mac-hid-dump`, `hidapi`, `IOKit HIDManager`, sito produttore

---

## 1. Identificazione componenti

### 1.1 Bridge USB (Bridgesil / 博捷半导体)

**Produttore**: 博捷半导体科技（苏州）有限公司 — Bridgesil Semiconductor Technology (Suzhou) Co., Ltd.
**Sito**: http://www.bridgesil.com.cn/
**Settore**: Circuiti integrati per interfacce USB ad alta velocità (hub, card reader, repeater, PD controller)

| Componente | VID | PID | Modello | Database USB-IF | Certezza |
|---|---|---|---|---|---|
| USB 3.2 Hub | `0x35D6` | `0x3510` | **BGS3510** | ❌ Non registrato | **Confermato** ✅ |
| USB 2.1 Hub | `0x35D6` | `0x2510` | **BGS2510** (presunto) | ❌ Non registrato | Probabile |

Il Vendor ID `0x35D6` non compare in database pubblici (linux-usb.org,
the-sz.com, devicehunt.com) perché Bridgesil non ha registrato il VID
presso USB-IF. I Product ID seguono la convenzione `0xNNNN` dove `NNNN`
è il numero del modello (es. `3510` = BGS**3510**).

#### BGS3510 — USB 3.2 Gen 1 4-Port Hub Controller

La pagina prodotto e il datasheet (912 KB, 14 pagine) sono disponibili
sul sito del produttore.

**Specifiche**:
- USB 3.2 Gen 1 (5 Gb/s) 4-porte downstream
- DFP 4A (Downstream Facing Port, 4 Ampere totali)
- BC1.2, Apple, Samsung charging support
- Interfaccia di configurazione **I2C** (registri accessibili via bus I2C)
- Package: **QFN-64** o **QFN-76**
- **Pin-to-pin compatibile con Genesys Logic GL3510**

> **⚠️ Riferimento incrociato**: Il GL3510 di Genesys Logic è un chip
> diffuso e ben documentato. Il datasheet GL3510 (pubblico) descrive
> la mappa dei registri I2C, la configurazione delle porte, il charging
> e le funzionalità della EEPROM. Essendo il BGS3510 dichiarato P2P
> compatibile, è probabile che erediti anche la compatibilità a livello
> di registri I2C. **Da verificare con test hardware.**

#### BGS3510S — Variante avanzata

Stesse specifiche del BGS3510 con aggiunte:
- USB ECN timer
- **Billboard HID Device** (per negoziazione USB-C Alternate Mode)
- Stesso package e piedinatura

#### BGS2510 — USB 2.1 Hub (presunto)

Il PID `0x2510` segue la convenzione BGS**2510**. Il modello non compare
nel catalogo online (sito aggiornato nel 2024, potrebbe elencare solo
i prodotti correnti). È verosimilmente il companion Hi-Speed del BGS3510,
probabilmente P2P compatibile con Genesys Logic GL850G o equivalente.

**Datasheet disponibili sul sito** (download center):
- BGS3510 datasheet: `/upload/20240815145503.pdf` (912 KB)
- BGS3510S datasheet: `/upload/20240815145448.pdf` (923 KB)
- BGS3523 datasheet: `/upload/20240815161203.pdf` (909 KB)

### 1.2 Hub USB 2.0 secondario (Terminus Technology)

| Componente | VID | PID | Database USB-IF | Certezza |
|---|---|---|---|---|
| USB 2.0 Hub | `0x1A40` | `0x0801` | ✅ Terminus Technology Inc. | **Confermato** |

**Terminus Technology Inc.** (Taiwan) è un produttore affermato di
controller USB hub. I loro chip noti includono:

| Modello | PID | Descrizione |
|---|---|---|
| FE1.1 | `0x0101` | USB 2.0 4-port Hub (diffusissimo) |
| FE2.1 | `0x0201` | USB 2.0 7-port Hub |

Il PID `0x0801` non corrisponde a nessun modello FE pubblico — è
probabilmente una variante custom o un modello più recente configurato
per single-port (1 downstream). Il driver macOS lo riconosce come
`AppleUSB20Hub`.

### 1.3 Controller HID

| Componente | VID | PID | Database USB-IF | Certezza |
|---|---|---|---|---|
| HID Controller | `0x05AC` | `0x029C` | ✅ Apple, Inc. (VID falso) | — |

Il controller HID usa il VID Apple per garantire il riconoscimento
automatico su macOS. Il manufacturer reale è "Bridgesil". Si tratta
con ogni probabilità di un microcontrollore generico (STM32, Nuvoton,
GD32 o simile) con firmware personalizzato. Impossibile identificare
il chip senza apertura fisica del dispositivo.

Il Report Descriptor (3 interfacce, 5 tipi di report, 437 byte totali)
non corrisponde a nessun profilo HID standard noto — è un design
custom.

### 1.4 Chip Audio

| Componente | VID | PID | Database USB-IF | Certezza |
|---|---|---|---|---|
| USB Audio | `0x0C76` | `0x1710` | ✅ JMTek, LLC. / Solid State System Co. | **Confermato** |

Il VID `0x0C76` appartiene a **JMTek, LLC.** (USA) / **Solid State
System Co., Ltd.** (Taiwan, www.3system.com.tw). JMTek produce
controller USB audio e mass storage.

Il PID `0x1710` non è nel database pubblico, ma JMTek ha altri
prodotti audio noti: `0x1607` ("audio controller"), `0x5663`
("Audio Device").

#### Stringa prodotto

La stringa `"USB PnP Audio Device(EEPROM)"` è fortemente indicativa
di un design basato su **C-Media** (VID `0x0D8C`), i cui chip HS-100,
CM108 e CM119 usano stringhe simili. Il suffisso "(EEPROM)" suggerisce
la presenza di una EEPROM esterna per la configurazione (PID/VID,
stringhe, descrittori). Tuttavia, non essendo il VID di C-Media,
potrebbe trattarsi di un design JMTek indipendente o di un rebranding
autorizzato. **Identificazione esatta non confermata senza ispezione
hardware.**

#### Interfacce USB Audio

Il chip espone 3 interfacce nel descrittore di configurazione:

| Interface | Class | SubClass | Protocollo | Endpoint | Ruolo |
|---|---|---|---|---|---|
| 0 | Audio (0x01) | Audio Control (0x01) | 0x00 | 0 | Controllo volume/mute |
| 1 | Audio (0x01) | Audio Streaming (0x02) | 0x00 | 0 (alt 0) | Streaming (inattivo a riposo) |
| 2 | Audio (0x01) | Audio Streaming (0x02) | 0x00 | 0 (alt 0) | Streaming (inattivo a riposo) |

Le interfacce di streaming hanno impostazioni alternate (alt 1) con
endpoint isocroni attivi durante la riproduzione/registrazione. Due
interfacce streaming suggeriscono supporto stereo output + mono input
(coerente con jack TRRS 3.5mm cuffie+microfono).

L'interfaccia HID Consumer Control (per Play/Pause) non appare come
interfaccia USB separata ma è gestita internamente dal chip.

#### Report Descriptor HID

```
05 0c — Usage Page (Consumer)
09 01 — Usage (Consumer Control)
a1 01 — Collection (Application)
09 cd — Play/Pause          ← bit 1
09 ea — Volume Decrement    ← bit 2
09 e9 — Volume Increment    ← bit 3
15 00 — Logical Min 0
25 01 — Logical Max 1
95 03 — Report Count 3
75 01 — Report Size 1
81 02 — INPUT (Data,Var,Abs)   — 3 bit di dati
95 05 — Report Count 5
81 03 — INPUT (Cnst,Var,Abs)   — 5 bit padding
c0     — End Collection
```

**Report: 1 byte** — 3 bit usati + 5 bit padding costanti.

| Bit | Maschera | Funzione |
|-----|----------|----------|
| 0 | `0x01` | Play/Pause |
| 1 | `0x02` | Volume Decrement |
| 2 | `0x04` | Volume Increment |
| 3–7 | `0xF8` | Padding (constant) |

> **⚠️ Discrepanza**: il descrittore dichiara Play/Pause sul bit 0
> (0x01), ma il monitoring live ha rilevato il bit 6 (0x40) in un
> report da 4 byte senza Report ID. Questo suggerisce che il chip
> audio possa inviare report in un formato diverso da quello
> dichiarato nel descrittore, oppure che il descrittore letto da
> ioreg appartenga a un'interfaccia diversa da quella effettivamente
> usata per il pulsante. Verifica hardware necessaria.

### 1.5 Riepilogo identificazione

| VID | PID | Vendor | Chip | Certezza | Datasheet |
|---|---|---|---|---|---|
| `0x35D6` | `0x3510` | Bridgesil (博捷) | **BGS3510** | **Confermato** ✅ | bridgesil.com.cn + GL3510 P2P |
| `0x35D6` | `0x2510` | Bridgesil (博捷) | BGS2510 (presunto) | Alta | — |
| `0x05AC` | `0x029C` | Apple (VID falso) | MCU custom firmware | Bassa | — |
| `0x1A40` | `0x0801` | Terminus Technology | Hub USB 2.0 custom | Media | — |
| `0x0C76` | `0x1710` | JMTek / Solid State | Audio USB + HID | Media | — |
| `—` | `—` | Bridgesil (博捷) | **BGS3224** card reader | Presunto | Non enumerato (slot vuoto) |

### 1.6 Prodotto commerciale

**Hagibis UC-1102AG** — USB-C Hub with Knob Shortcut Buttons.

Produttore: Hagibis (Shanghai). Il dispositivo è un prodotto OEM: il brand
è Hagibis, i chip interni sono Bridgesil (hub + card reader), Terminus
(hub USB 2.0), JMTek (audio), più un MCU custom per i pulsanti HID.

---

## 2. Topologia USB

```
USB-C upstream (connesso al Mac)
 │
 ├── AppleT8132USBXHCI@01000000 (SuperSpeed USB 3.1 bus)
 │    │
 │    └── USB3.2 Hub@00200000          ← Bridge chip lato SuperSpeed
 │         VID: 0x35D6  PID: 0x3510
 │         Manufacturer: Bridgesil
 │         Version: 0x0108
 │         Link Speed: 5 Gb/s
 │         bDeviceClass: 9 (Hub)
 │         └── 4 downstream ports USB 3.x (2× USB-A 3.x, 1× USB-C, 1× non esposta)
 │
 └── AppleT8132USBXHCI@00000000 (Hi-Speed USB 2.0 companion bus)
      │
      └── USB2.1 Hub@00100000          ← Bridge chip lato Hi-Speed
           VID: 0x35D6  PID: 0x2510
           Manufacturer: Bridgesil
           Version: 0x0108
           Link Speed: 480 Mb/s
           bDeviceClass: 9 (Hub)
           │
           ├── Hid Device@00150000     ← Controller HID (pulsanti + knob)
           │    VID: 0x05AC  PID: 0x029C
           │    Manufacturer: Bridgesil
           │    Version: 0x0108
           │    Link Speed: 480 Mb/s
           │    Power: 12 W sink (2400 mA), 2.5 W allocated (500 mA)
           │    Hardware Type: Non-removable
           │    Espone 3 interfacce HID (vedi §5)
           │
           └── USB 2.0 Hub@00140000    ← Hub secondario USB 2.0
                VID: 0x1A40  PID: 0x0801
                Version: 0x0100
                Link Speed: 480 Mb/s
                └── 1 downstream port USB 2.0
                     │
                     └── USB PnP Audio Device(EEPROM)@00142000
                          VID: 0x0C76  PID: 0x1710
                          Version: 0x0100
                          Link Speed: 12 Mb/s (Full Speed)
                          Hardware Type: Removable
                          Chip audio USB — jack cuffie 3.5mm + consumer control
```

### 2.1 Hub USB3.2 (upstream-facing: SuperSpeed)

Il bridge `0x35D6:0x3510` è un **USB 3.2 Gen 1 Hub** (5 Gb/s) con 4 porte
downstream. Gestisce il traffico SuperSpeed (5 Gb/s) verso le periferiche
USB 3.x collegate alle porte downstream.

| Proprietà | Valore |
|---|---|
| Vendor ID | `0x35D6` |
| Product ID | `0x3510` |
| bDeviceClass | 9 (Hub) |
| Link Speed | 5 Gb/s |
| Downstream ports | 4 (USB 3.x) |
| Manufacturer | Bridgesil |
| Version | 0x0108 |

### 2.2 Hub USB2.1 (upstream-facing: Hi-Speed)

Il bridge `0x35D6:0x2510` è un **USB 2.1 Hub** (480 Mb/s). Contiene due
dispositivi a valle fissi (non removibili via hub):
- il controller HID (`0x05AC:0x029C`)
- un hub USB 2.0 secondario (`0x1A40:0x0801`)

| Proprietà | Valore |
|---|---|
| Vendor ID | `0x35D6` |
| Product ID | `0x2510` |
| bDeviceClass | 9 (Hub) |
| Link Speed | 480 Mb/s |
| Manufacturer | Bridgesil |
| Version | 0x0108 |

### 2.3 Hub USB 2.0 secondario

Hub a valle del USB2.1 Hub. Espone 1 porta downstream a cui è connesso
il chip audio.

| Proprietà | Valore |
|---|---|
| Vendor ID | `0x1A40` |
| Product ID | `0x0801` |
| bDeviceClass | 9 (Hub) |
| Link Speed | 480 Mb/s |
| Version | 0x0100 |
| Downstream ports | 1 (USB 2.0) |

### 2.4 Porte fisiche del dispositivo

Il dispositivo espone all'esterno (verificato da sito produttore + live):

| Porta | Tipo | Bus | Velocità max | Descrizione |
|---|---|---|---|---|
| Upstream | USB-C | — | 10 Gb/s (?) | Connessione al computer host |
| Downstream 1 | USB-A | USB 3.2 Hub | 5 Gb/s | USB 3.x dati |
| Downstream 2 | USB-A | USB 3.2 Hub | 5 Gb/s | USB 3.x dati |
| Downstream 3 | USB-C | USB 3.2 Hub | 5 Gb/s | USB 3.x dati + possibile PD pass-through |
| Microfono | Jack 3.5mm | USB 2.0 Hub → Audio chip | — | Input microfono |
| Cuffie | Jack 3.5mm | USB 2.0 Hub → Audio chip | — | Output stereo |
| Headset | Jack 3.5mm | USB 2.0 Hub → Audio chip | — | Combo TRRS cuffie+microfono |
| SD | SD 3.0 UHS-I | Interno | ~100 MB/s | Lettore SD (solo se scheda inserita) |
| TF | microSD 3.0 UHS-I | Interno | ~100 MB/s | Lettore microSD (solo se scheda inserita) |
| Downstream (interna) | — | USB 2.0 Hub | 480 Mb/s | Non esposta; usata dal chip audio |

> **Nota sul lettore SD/TF**: Il bridge BGS3510 di Bridgesil supporta un
> controller card-reader (BGS3224, vedi §1). Il lettore appare nello
> USB tree **solo quando una scheda è inserita** — a vuoto non viene
> enumerato. Test con scheda inserita necessario per verificare VID/PID.
> Stessa cosa per il lettore TF (microSD), indipendente.

### 2.5 RGB ambient light (LED knob)

Il dispositivo ha un LED RGB integrato nel knob. Il pulsante basso-sinistra
(nessun report HID) cicla i colori/pattern del LED. Questa funzione è
gestita interamente dal firmware del controller HID e non è accessibile
via USB. I colori sono puramente estetici.

---

## 3. Identificativi

| Componente | VID | PID | Vers. | bDeviceClass | Ruolo |
|---|---|---|---|---|---|
| Bridge SuperSpeed | `0x35D6` | `0x3510` | 0x0108 | 9 (Hub) | USB 3.2 Hub 4-porte |
| Bridge Hi-Speed | `0x35D6` | `0x2510` | 0x0108 | 9 (Hub) | USB 2.1 Hub interno |
| **Controller HID** | **`0x05AC`** | **`0x029C`** | 0x0108 | 0 (per-interface) | Pulsanti + knob |
| Hub secondario | `0x1A40` | `0x0801` | 0x0100 | 9 (Hub) | Hub USB 2.0 per audio |
| Chip audio | `0x0C76` | `0x1710` | 0x0100 | 0 (per-interface) | Jack 3.5mm + consumer control |

> **Nota sul VID Apple** (`0x05AC`): il controller HID usa il Vendor ID di
> Apple per garantire il riconoscimento plug-and-play su macOS senza driver
> aggiuntivi. È un VID falso (il manufacturer è Bridgesil, non Apple).
> Qualsiasi software che enumera dispositivi HID deve filtrare per
> `product_string == "Hid Device"` o per `manufacturer == "Bridgesil"`
> per non confonderlo con periferiche Apple reali.

---

## 4. Chip Audio — USB PnP Audio Device

### 4.1 Specifiche

| Proprietà | Valore |
|---|---|
| Vendor ID | `0x0C76` |
| Product ID | `0x1710` |
| Link Speed | 12 Mb/s (Full Speed) |
| Version | 0x0100 |
| Tipo | USB Audio Class + HID Consumer Control |
| Connessione | Jack 3.5mm TRRS (cuffie + microfono) |

Il chip audio è connesso a valle dell'hub USB 2.0 secondario (`0x1A40:0x0801`).
Opera a Full Speed (12 Mb/s).

### 4.2 Interfacce esposte

Il chip espone almeno due interfacce USB:

1. **Audio Streaming** — USB Audio Class 1.0/2.0 per riproduzione e
   registrazione audio via jack 3.5mm
2. **HID Consumer Control** — consumer page (0x0C:0x01) per tasti media.
   Usato per il pulsante **Play/Pause** (vedi §6.2)

### 4.3 Consumer Control HID

Il report consumer del chip audio ha lo stesso formato dell'Interface 1 del
controller HID: 4 byte, nessun Report ID, Consumer Page (0x0C).

| Bit | Maschera | Funzione |
|-----|----------|----------|
| 0 | `0x01` | Volume Increment (non usato su questo chip) |
| 1 | `0x02` | Volume Decrement (non usato) |
| 2 | `0x04` | Mute (non usato) |
| 3 | `0x08` | — |
| 4 | `0x10` | Stop |
| 5 | `0x20` | Scan Next Track |
| 6 | `0x40` | **Play/Pause** |
| 7 | `0x80` | Scan Prev Track |

> **Nota**: i bit 0–3 (volume, mute) non sono cablati su questo chip.
> Il pulsante fisico Play/Pause invia il bit 6 (`0x40`) come Absolute
> (non Relative come da descriptor del controller HID).

---

## 5. Controller HID — Interfacce e Report

Il controller HID (`0x05AC:0x029C`) espone **3 interfacce HID**.
L'interfaccia 2 è **composite**: il suo descriptor contiene 3 collection HID
distinte accessibili tramite un unico endpoint.

### 5.1 Enumeration

```
Interface 0:  usage=0x0001:0x0006 (Keyboard)         — Standard 6KRO
Interface 1:  usage=0x000C:0x0001 (Consumer Control)  — Knob
Interface 2:  usage=0x0001:0x0006 (Keyboard)          — Composite, collection 1
Interface 2:  usage=0x000C:0x0001 (Consumer Control)  — Composite, collection 2
Interface 2:  usage=0xFF00:0x0006 (Vendor-Defined)    — Composite, collection 3
```

Aprendo il device HID per Interface 2 si ricevono report con Report ID
che identificano la collection di appartenenza.

### 5.2 Interface 0 — Standard Keyboard 6KRO

**Report Descriptor**: 67 byte. Nessun Report ID.

**Report (8 byte)**:

| Byte | Bit | Campo |
|------|-----|-------|
| 0 | 0 | Left Control |
| 0 | 1 | Left Shift |
| 0 | 2 | Left Alt |
| 0 | 3 | Left GUI (Command) |
| 0 | 4 | Right Control |
| 0 | 5 | Right Shift |
| 0 | 6 | Right Alt |
| 0 | 7 | Right GUI |
| 1 | — | Riservato (0x00) |
| 2–7 | — | Keycode array 6KRO (6 byte, 0x00 = nessun tasto) |

**Output (host → device)**: 1 byte — LED status (Num Lock, Caps Lock,
Scroll Lock, Compose, Kana) su 5 bit + 3 bit padding.

```
Descriptor (67 byte):
  05 01        Usage Page (Generic Desktop)
  09 06        Usage (Keyboard)
  a1 01        Collection (Application)
  05 07        Usage Page (Keyboard/Keypad)
  19 e0        Usage Minimum (0xE0 = Left Control)
  29 e7        Usage Maximum (0xE7 = Right GUI)
  15 00        Logical Minimum (0)
  25 01        Logical Maximum (1)
  95 08        Report Count (8)
  75 01        Report Size (1 bit)
  81 02        INPUT (Data,Var,Abs) → Modifier bitmap
  95 01        Report Count (1)
  75 08        Report Size (8 bit)
  81 03        INPUT (Const,Var,Abs) → Reserved byte
  05 07        Usage Page (Keyboard/Keypad)
  95 06        Report Count (6)
  75 08        Report Size (8 bit)
  15 00        Logical Minimum (0)
  26 ff 00     Logical Maximum (255)
  19 00        Usage Minimum (0)
  2a ff 00     Usage Maximum (255)
  81 00        INPUT (Data,Array,Abs) → Keycode array 6KRO
  05 08        Usage Page (LEDs)
  25 01        Logical Maximum (1)
  95 05        Report Count (5)
  75 01        Report Size (1 bit)
  19 01        Usage Minimum (1 = Num Lock)
  29 05        Usage Maximum (5 = Kana)
  91 02        OUTPUT (Data,Var,Abs) → LED status
  95 01        Report Count (1)
  75 03        Report Size (3 bit)
  91 03        OUTPUT (Const,Var,Abs) → Padding
  c0           End Collection
```

### 5.3 Interface 1 — Consumer Control / Knob

**Report Descriptor**: 60 byte. Nessun Report ID.

**Report (4 byte)**:

| Byte | Bit | Campo | Tipo |
|------|-----|-------|------|
| 0 | 0 | Volume Increment (0xE9) | **Absolute** |
| 0 | 1 | Volume Decrement (0xEA) | **Absolute** |
| 0 | 2 | Mute (0xE2) | **Relative** |
| 0 | 3 | Unassigned (0x00) | Relative |
| 0 | 4 | Stop (0xB7) | Relative |
| 0 | 5 | Scan Next Track (0xB5) | Relative |
| 0 | 6 | Scan Prev Track (0xB6) | Relative |
| 0 | 7 | Play/Pause (0xCD) | Relative |
| 1 | — | Riservato | — |
| 2 | — | Riservato | — |
| 3 | — | Riservato | — |

**Nota sul tipo Absolute vs Relative**:

| Campo | Tipo | Comportamento |
|---|---|---|
| VolInc, VolDec | Absolute | Bit = 1 mentre il knob ruota, 0 quando fermo |
| Mute, Stop, Next, Prev, Play | Relative | Ogni transizione 0→1 genera un evento cumulativo |

> **⚠️ Nota**: i bit 4–7 (Stop, Next, Prev, Play/Pause) nel descriptor
> dell'Interface 1 non sono cablati a pulsanti fisici sul controller HID —
> il knob produce solo Vol+/Vol-/Mute (bit 0–2). Il pulsante Play/Pause
> reale si trova sul chip audio (vedi §6.1) e invia il bit 6 (`0x40`).
> Inoltre, su questo specifico esemplare di firmware, i bit 6 e 7 risultano
> invertiti rispetto al descriptor: il descriptor dichiara bit 6 = Prev,
> bit 7 = Play/Pause, ma il device invia Play/Pause su bit 6 e Prev su bit 7.
> Qualsiasi implementazione deve verificare il mapping effettivo con un
> test live.

```
Descriptor (60 byte):
  05 0c        Usage Page (Consumer)
  09 01        Usage (Consumer Control)
  a1 01        Collection (Application)
  15 00        Logical Minimum (0)
  25 01        Logical Maximum (1)
  09 e9        Usage (Volume Increment)
  09 ea        Usage (Volume Decrement)
  75 01        Report Size (1 bit)
  95 02        Report Count (2)
  81 02        INPUT (Data,Var,Abs) → VolInc, VolDec
  09 e2        Usage (Mute)
  09 00        Usage (Unassigned)
  95 02        Report Count (2)
  81 06        INPUT (Data,Var,Rel) → Mute, Unassigned
  05 0c        Usage Page (Consumer)
  09 b7        Usage (Stop)
  09 b5        Usage (Scan Next Track)
  09 b6        Usage (Scan Previous Track)
  09 cd        Usage (Play/Pause)
  95 04        Report Count (4)
  81 06        INPUT (Data,Var,Rel) → Media buttons
  26 ff 00     Logical Maximum (255)
  09 00        Usage (Unassigned)
  75 08        Report Size (8 bit)
  95 03        Report Count (3)
  81 02        INPUT (Data,Var,Abs) → Riservati
  09 00        Usage (Unassigned)
  95 04        Report Count (4)
  91 02        OUTPUT (Data,Var,Abs) → (non documentato)
  c0           End Collection
```

### 5.4 Interface 2 — Composite

**Report Descriptor**: 207 byte. Contiene 3 collection con Report ID.

#### Collection 1: Keyboard 6KRO (Report ID 0x01)

**Report (10 byte)**:

| Byte | Bit | Campo |
|------|-----|-------|
| 0 | — | **Report ID = 0x01** |
| 1 | 0 | Left Control |
| 1 | 1 | Left Shift |
| 1 | 2 | Left Alt |
| 1 | 3 | Left GUI (Command) |
| 1 | 4 | Right Control |
| 1 | 5 | Right Shift |
| 1 | 6 | Right Alt |
| 1 | 7 | Right GUI |
| 2 | — | Riservato (0x00) |
| 3–8 | — | Keycode array 6KRO (6 byte) |
| 9 | 0 | Eject (Consumer 0x0C:0xB8) |
| 9 | 1 | Vendor-defined (0xFF:0x03) |
| 9 | 2 | Menu (Consumer 0x0C:0x40) |
| 9 | 3 | AL Terminal Lock / Screensaver (0x0C:0x19E) |
| 9 | 4–7 | Padding |

**Output (host → device)**: 5 bit LED status + 3 bit padding.

#### Collection 2: Media Controls (Report ID 0x52)

**Report (2 byte)**:

| Byte | Bit | Campo |
|------|-----|-------|
| 0 | — | **Report ID = 0x52** |
| 1 | 0 | Play/Pause (Consumer 0xCD) |
| 1 | 1 | Fast Forward (Consumer 0xB3) |
| 1 | 2 | Rewind (Consumer 0xB4) |
| 1 | 3 | Scan Next Track (Consumer 0xB5) |
| 1 | 4 | Scan Prev Track (Consumer 0xB6) |
| 1 | 5–7 | Padding |

#### Collection 3: Vendor-Defined (Report ID 0x3F)

**Report (65 byte)**: 1 byte Report ID + 64 byte dati proprietari.
Usage Page `0xFF00`, Usage `0x06`. Presumibilmente usato per
configurazione firmware e diagnostica interna.

---

## 6. Pulsanti fisici e comportamento predefinito

Il dispositivo ha **4 pulsanti fisici** e un **knob rotante con click**.
I pulsanti hanno un comportamento predefinito cablato nel firmware:

| # | Posizione | Funzione predefinita | Report HID | Note |
|---|---|---|---|---|
| 1 | Alto-Sinistra | **Lock Screen** | Keyboard (Interface 0) | Press breve e hold inviano due combinazioni distinte |
| 2 | Basso-Sinistra | Cicla LED knob | **Nessuno** | Firmware-only, non produce report verso l'host |
| 3 | Alto-Destra | **Play/Pause** | Consumer (chip audio) | Sul chip audio `0x0C76:0x1710`, bit 0x40 |
| 4 | Basso-Destra | **Screenshot** | Keyboard (Interface 0) | Press breve e hold inviano due combinazioni distinte |

I pulsanti **alto-sinistra** e **basso-destra** sono dual-function:
il firmware rileva la durata della pressione e invia una combinazione
di tasti diversa per press breve vs. hold (>500ms).

### 6.1 Combinazioni keystroke (verificate via monitoring live)

| Pulsante | Trigger | Keycode | Mod. HID | Combinazione |
|---|---|---|---|---|
| Alto-Sinistra | Press breve | `0x20` | `0x01` (L Ctrl) | Ctrl+3 |
| Alto-Sinistra | Hold | `0x46` | `0x08` (L GUI) | Cmd+F13 |
| Basso-Destra | Press breve | `0x14` | `0x01` (L Ctrl) | Ctrl+Q |
| Basso-Destra | Hold | `0x0F` | `0x08` (L GUI) | Cmd+R |

> **Nota**: le combinazioni osservate (Ctrl+Q, Ctrl+3, Cmd+F13, Cmd+R)
> potrebbero non corrispondere esattamente alle funzioni "lock screen" e
> "screenshot" come percepite dall'utente — macOS interpreta queste
> combinazioni in base al contesto applicativo. Il firmware del hub si
> limita a inviare i keystroke; l'effetto finale dipende dal sistema
> operativo host.

### 6.2 Play/Pause (alto-destra, chip audio)

| Dispositivo | Bit | Tipo | Comportamento |
|---|---|---|---|
| Chip audio (`0x0C76:0x1710`) | 6 (`0x40`) | Absolute | 1 premuto, 0 rilasciato |

> Il pulsante Play/Pause è l'unico controllo connesso al chip audio.
> Invia un report Consumer Control (4 byte, nessun Report ID) con il
> bit 6 impostato. Non appare nei report del controller HID.

### 6.3 LED knob (basso-sinistra, nessun report)

Il pulsante basso-sinistra è cablato direttamente al firmware del
controller HID e **non produce alcun report verso l'host**. La sua
funzione è ciclare l'illuminazione del LED integrato nel knob
(off → colore 1 → colore 2 → ...). Non è rilevabile via USB/HID.

### 6.4 Knob rotante con click

Il knob è connesso all'Interface 1 del controller HID. Supporta
rotazione (volume) e pressione (mute/unmute).

| Azione | Bit | Tipo | Comportamento |
|---|---|---|---|
| Rotazione destra | 0 (`0x01`) | Absolute | 1 mentre ruota, 0 a riposo |
| Rotazione sinistra | 1 (`0x02`) | Absolute | 1 mentre ruota, 0 a riposo |
| Click (mute/unmute) | 2 (`0x04`) | Relative | Ogni click genera un evento |

### 6.5 Riepilogo controlli

| Controllo | Posizione | Tipo report | Evento |
|---|---|---|---|
| Lock Screen | Alto-Sinistra | Keyboard 6KRO | Ctrl+3 (press) / Cmd+F13 (hold) |
| LED knob | Basso-Sinistra | — | Nessun report |
| Play/Pause | Alto-Destra | Consumer (audio chip) | Bit 0x40 |
| Screenshot | Basso-Destra | Keyboard 6KRO | Ctrl+Q (press) / Cmd+R (hold) |
| Knob rotate | — | Consumer (Interface 1) | Vol+ / Vol- (Absolute) |
| Knob click | — | Consumer (Interface 1) | Mute (Relative) |

---

## 7. Riepilogo report HID

| Sorgente | Report ID | Dimensione | Contenuto |
|---|---|---|---|
| Controller HID, Interface 0 | Nessuno | 8 byte | Keyboard 6KRO (modifiers + 6 keycode) |
| Controller HID, Interface 1 | Nessuno | 4 byte | Knob (Vol±, Mute, media bits) |
| Controller HID, Interface 2 | `0x01` | 10 byte | Keyboard 6KRO con extras (Eject, Menu, Screensaver) |
| Controller HID, Interface 2 | `0x52` | 2 byte | Media keys (Play, FF, Rew, Next, Prev) |
| Controller HID, Interface 2 | `0x3F` | 65 byte | Vendor data (1 ID + 64 byte) |
| Chip audio, Consumer Control | Nessuno | 4 byte | Consumer Control (Play/Pause su bit 6) |

---

## 8. Note per lo sviluppo

### 8.1 Accesso ai report

- Il controller HID e il chip audio sono dispositivi **separati** sull'albero
  USB. Per intercettare tutti gli eventi occorre ascoltare entrambi
  (`0x05AC:0x029C` per pulsanti + knob, `0x0C76:0x1710` per Play/Pause).
- Interface 0 e Interface 2 inviano report tastiera con formato 6KRO:
  fino a 6 tasti simultanei. I tasti premuti appaiono come keycode non-zero
  nei byte 2–7 (o 3–8 per Report ID 0x01); i tasti rilasciati scompaiono
  dall'array (diventano 0x00).
- L'Interface 1 (Consumer Control) usa report Absolute per la rotazione del
  knob (VolInc/VolDec) e Relative per i tasti media (Mute, Stop, Next, Prev,
  Play). Per i campi Relative, ogni transizione 0→1 è un evento.
- Il Report ID è incluso nel buffer come primo byte quando presente.
  `data[0]` per report senza ID; `data[1]` per il payload di report con ID.

### 8.2 Compatibilità macOS

- Il VID Apple (`0x05AC`) sul controller HID attiva il driver
  `AppleHIDKeyboardEventDriverV2` su macOS, che mappa i report tastiera
  in eventi di sistema. L'Interface 1 (Consumer Control) viene gestita
  da `AppleUserHIDEventService`.
- Il chip audio (`0x0C76:0x1710`) è riconosciuto come periferica
  USB Audio Class standard e appare in `System Settings → Sound`.
- Su macOS, l'accesso ai report HID senza sottrarli al sistema richiede
  IOKit con `kIOHIDOptionsTypeNone` (apertura non-esclusiva). L'API
  CGEventTap (Quartz) non riceve gli eventi Consumer Control generati
  dall'Interface 1 perché il driver HID li converte in azioni di sistema
  prima che raggiungano il Quartz Event System.
- L'accesso esclusivo (es. `hidapi` con `O_RDWR`) sottrae il device al
  driver di sistema e ne interrompe il funzionamento normale.

### 8.3 Driver Apple rilevanti

Il dispositivo appare in `ioreg` con i seguenti driver:

- `AppleHIDKeyboardEventDriverV2` (×2 — Interface 0 e Interface 2 keyboard)
- `AppleUserHIDEventService` (Interface 1 Consumer Control)
- `AppleUserHIDEventService` (Chip audio Consumer Control)
- `AppleUSBHostCompositeDevice` (Controller HID)
- `AppleUSBAudioDevice` (Chip audio)
