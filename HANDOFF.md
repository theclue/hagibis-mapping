# Rust FFI Handoff — Problemi di compilazione e soluzioni

## 1. `extern` blocks must be `unsafe` (edition 2024)

**Sintomo**: `error: extern blocks must be unsafe`

**Causa**: Rust 2024 edition richiede che TUTTI i blocchi `extern "C"` siano marcati `unsafe extern "C"`.

**Fix**: Cambiato ogni `extern "C" { }` in `unsafe extern "C" { }`.

---

## 2. `core-foundation` crate — tipi privati e trait bounds

**Sintomo**:
```
error[E0603]: type alias import `CFStringRef` is private
error[E0277]: the trait bound `*const c_void: TCFType` is not satisfied
error[E0038]: the trait `TCFType` is not dyn compatible
```

**Causa**: Il crate `core-foundation` v0.10 marca i type alias interni come privati. I tipi `CFStringRef`, `CFNumberRef`, `CFDictionaryRef` NON sono esportati pubblicamente. Inoltre, `from_CFType_pairs` richiede tipi concreti, non `&dyn TCFType`.

**Fix**: Rimosso `core-foundation` e `core-graphics` come dipendenze. Tutti i tipi CF definiti manualmente come type alias di `*const std::ffi::c_void`:

```rust
pub type CFStringRef = *const std::ffi::c_void;
pub type CFNumberRef = *const std::ffi::c_void;
pub type CFDictionaryRef = *const std::ffi::c_void;
```

I dizionari CF sono costruiti con chiamate raw FFI a `CFDictionaryCreate`, `CFArrayCreate`, ecc.

---

## 3. Linker — framework IOKit e CoreGraphics non trovati

**Sintomo**: `error: linking with cc failed: exit status: 1` — undefined symbols per `IOHIDManagerCreate`, `CGEventCreateKeyboardEvent`, etc.

**Causa**: Rust non linka automaticamente i framework di sistema macOS. Le funzioni IOKit e CoreGraphics sono in `/System/Library/Frameworks/IOKit.framework` e `CoreGraphics.framework`, ma il linker non le cerca.

**Fix**: Creato `build.rs` con le direttive di linking:

```rust
fn main() {
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=IOKit");
        println!("cargo:rustc-link-lib=framework=CoreGraphics");
    }
}
```

Tentativo fallito: `#[link(name = "IOKit", kind = "framework")]` su un blocco `extern "C" {}` vuoto nel file FFI. Non funziona in Rust 2024 — gli attributi `#[link]` su blocchi extern vuoti non sono supportati. Solo `build.rs` risolve.

---

## 4. Multithreading — `*mut c_void` non è `Send`

**Sintomo**: `error[E0277]: *mut c_void cannot be sent between threads safely`

**Causa**: In Rust, i raw pointer (`*mut T`, `*const T`) NON implementano il trait `Send`. Se una `thread::spawn(move || { ... })` cattura un raw pointer nella closure, la compilazione fallisce.

**Tentativi falliti**:

1. **`unsafe impl Send for IOKitManager {}`** — Non funziona perché il problema non è il tipo `IOKitManager`, ma i raw pointer LOCALI (`let mgr = self.manager`) catturati dalla closure.

2. **Newtype `SendPtr<T>` wrapper**:
   ```rust
   struct SendPtr<T: Copy>(T);
   unsafe impl<T: Copy> Send for SendPtr<T> {}
   ```
   La closure cattura `SendPtr<*mut c_void>`, che dovrebbe essere `Send`, ma il compilatore continua a rifiutare. Il motivo è che quando la closure accede a `mgr_send.0` (unwrap del raw pointer), il raw pointer "nudo" appare nel corpo della closure e il compilatore lo considera non-Send.

3. **`NonNull<c_void>`** — Anche `NonNull<T>` NON implementa `Send`.

**Root cause**: Il compilatore di Rust è estremamente conservativo con i raw pointer nei thread. Anche avvolgendoli in newtype marcati `Send`, se il pointer viene poi "estratto" via `.0` e usato nel corpo della closure, il type checker lo rileva come non-inviabile.

**Fix finale**: **Eliminato completamente il threading.** Invece di `thread::spawn`, il metodo `start_reading` è stato sostituito con `run_once(timeout_ms)`. Il chiamante (main thread) esegue un loop:

```rust
while running() {
    match seize_backend.run_once(50)? {
        Some(report) => { /* dispatch */ }
        None => { /* timeout, riprova */ }
    }
}
```

La callback C (`hid_report_collector`) pusha i report in una `Arc<Mutex<VecDeque<Report>>>`. `run_once` esegue `CFRunLoopRunInMode(timeout, true)` e poi drena la coda. Zero thread, zero Send issue.

---

## 5. MacOS Seize con matching prima dell'open

**Sintomo**: `IOHIDManagerOpen` con `kIOHIDOptionsTypeSeizeDevice` fallisce con `kIOReturnNotPrivileged` (0xE00002C1) quando il matching è impostato PRIMA dell'open.

**Causa**: Quando `IOHIDManagerOpen` viene chiamato con matching già impostato, tenta di sequestrare immediatamente i dispositivi. Senza `sudo`, il processo non ha i permessi per il seize. Con `sudo`, funziona.

**Fix**: L'ordine corretto è:
1. `IOHIDManagerCreate`
2. `IOHIDManagerScheduleWithRunLoop`
3. `IOHIDManagerOpen` (con seize) — senza matching, restituisce 0
4. `IOHIDManagerSetDeviceMatching` — i dispositivi che matchano vengono seized

Con matching prima dell'open + sudo, restituisce 0 e il seize funziona correttamente. La versione finale del codice usa matching prima dell'open.

---

## 6. Brace mismatch da edit cumulativi

**Sintomo**: `error: unexpected closing delimiter: }` con indicazione di brace non bilanciate.

**Causa**: Durante le modifiche cumulative, un `unsafe { ... };}` ha accumulato un `;}` residuo dove doveva esserci solo `;`.

**Fix**: Pulizia manuale.

---

## 7. `kCFRunLoopDefaultMode` come simbolo esportato

**Sintomo**: Impossibile ottenere `kCFRunLoopDefaultMode` come `CFStringRef` senza dipendere da `core-foundation`.

**Causa**: `kCFRunLoopDefaultMode` è un simbolo esportato da CoreFoundation. Non è accessibile come costante Rust senza `extern` link.

**Fix**:
```rust
pub fn kCFRunLoopDefaultMode() -> CFStringRef {
    unsafe extern "C" {
        #[link_name = "kCFRunLoopDefaultMode"]
        static MODE: CFStringRef;
    }
    unsafe { MODE }
}
```

---

## Riepilogo dipendenze finali

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
toml = "0.8"
ctrlc = "3"
```

Nessuna dipendenza da `core-foundation`, `core-graphics`, `enigo`, `keyseq`, `hidapi`. Solo Rust std + 3 crate per serializzazione e segnali.

## Architettura finale

```
main.rs → cli::run()
           ├── manager::load_or_default() → Config da TOML
           ├── IOKitManager::seize()      → IOKit FFI, matching dict, callback C
           ├── loop { run_once(50ms) }    → CFRunLoopRunInMode, drain queue
           │    └── Dispatcher::dispatch() → parse → inject
           └── release()                  → unschedule, close
```

Zero thread. Tutto sul main thread, come monitor.py.

---

# AUDIT — Bug runtime trovati e corretti (post-compilazione)

Il codice **compilava** ma conteneva bug di memoria/FFI che il compilatore Rust NON
rileva (sono in blocchi `unsafe`). Confronto fatto contro il PoC Python `probe/seize.py`,
che è la reference funzionante. Tutti corretti.

## BUG #1 (CRITICO) — Context pointer type mismatch [seize.rs]

`IOHIDManagerRegisterInputReportCallback` riceve un context `*mut c_void`. Il codice faceva:

```rust
let ctx = Box::into_raw(Box::new(reports)) as *mut c_void;  // *mut Arc<Mutex<VecDeque>>
```

ma la callback lo reinterpretava come tipo diverso:

```rust
let reports = &*(context as *const Mutex<VecDeque<Report>>);  // ✗ era un Arc, non un Mutex!
```

Un `Arc<T>` è un handle (puntatore a `ArcInner` con i contatori), NON un `T`. Dereferenziare
i byte dell'Arc come se fossero un `Mutex` = **undefined behavior / corruzione di memoria** al
primo report HID.

**Fix**: `Arc::into_raw(self.reports.clone())` restituisce direttamente `*const Mutex<VecDeque<Report>>`
(punta al T interno), che è esattamente ciò che la callback interpreta. Puntatore salvato in
`IOKitManager.ctx_ptr` e recuperato in `release()` con `Arc::from_raw` (niente leak).

## BUG #2 (CRITICO) — CF container con callback NULL + release prematuro [seize.rs]

`CFDictionaryCreate`/`CFArrayCreate` venivano chiamati con callback NULL per key/value/element:

```rust
CFDictionaryCreate(.., std::ptr::null(), std::ptr::null());  // ✗ niente retain
```

Con callback NULL, CoreFoundation **non fa retain** degli elementi. Subito dopo il codice
faceva `CFRelease` di tutte le chiavi/valori/dizionari → il container restava con **puntatori
dangling** → use-after-free quando IOKit legge il matching.

**Fix**: passare gli static esportati `kCFTypeDictionaryKeyCallBacks`,
`kCFTypeDictionaryValueCallBacks`, `kCFTypeArrayCallBacks` (esposti in `ffi.rs` come tipo opaco
+ accessor). Ora CF fa retain e il `CFRelease` locale è corretto. Aggiunto anche `CFRelease`
dell'array di matching dopo `SetDeviceMatchingMultiple` (IOKit ne copia i criteri).

## BUG #3 (ALTO) — CGEventPost dichiarato con valore di ritorno [ffi.rs + inject.rs]

`CGEventPost` nella vera API CoreGraphics è `void`. Era dichiarato:

```rust
pub fn CGEventPost(tap: u32, event: *mut c_void) -> *mut c_void;  // ✗
```

e `inject.rs` controllava `if posted.is_null()`. Leggere il valore di ritorno di una funzione
`void` significa leggere un **registro indefinito** → il check falliva/passava a caso (è il
"false negative" già annotato nel PoC Python). 

**Fix**: dichiarato `CGEventPost` senza ritorno; rimossi i check su `posted`. Il successo è
implicito dalla creazione non-NULL dell'evento (come fa il Python).

## BUG #4 (ALTO) — Matching impostato DOPO open [seize.rs]

L'ordine era `open(seize)` → `SetDeviceMatchingMultiple`. Il PoC Python (funzionante) imposta
il matching **PRIMA** di open: i device che matchano sono quelli che vengono seized. Con open
prima del matching si rischia di non sequestrare nulla (o tentare di sequestrare ogni device HID).

**Fix**: riordinato a `SetDeviceMatchingMultiple` → `open(seize)` → `RegisterCallback`, identico
al Python.

## BUG #5 (MEDIO) — CGEvent mai rilasciati [inject.rs]

Ogni `CGEventCreateKeyboardEvent` crea un oggetto CF con retain count 1. In Rust non c'è
autorelease pool → ogni tasto iniettato perdeva memoria.

**Fix**: `CFRelease(event)` dopo ogni `CGEventPost`.

## BUG #6 (MEDIO) — Box del context mai liberato [seize.rs]

Risolto insieme a #1: il context è ora un `Arc` recuperato con `Arc::from_raw` in `release()`.

## BUG #7 (BASSO) — Source NULL invece di HIDSystemState [inject.rs]

Il Python crea un `CGEventSourceCreate(kCGEventSourceStateHIDSystemState)`; il Rust passava
NULL. NULL è accettato ma la source HID è più robusta per i flag modificatori.

**Fix**: `CGEventInjector` ora crea una source una volta in `new()`, la riusa, e la rilascia
nel `Drop`. Se la creazione fallisce (NULL) il comportamento degrada a quello precedente.

## BUG #8 (BASSO) — kCGSessionEventTap valore errato [ffi.rs]

`kCGSessionEventTap` era `= 0`, ma nell'enum `CGEventTapLocation` lo `0` è `kCGHIDEventTap`;
`kCGSessionEventTap` vale `1`. Il Python posta su session tap.

**Fix**: costante corretta a `1`.

## Note correttezza verificate (NON bug)

- Parser report (`parser.rs`): branch `report_id == 0` con len≥8→keyboard (mod=`data[0]`,
  keys=`data[2..8]`), len≥4→consumer (`data[0]`) — coincide con il Python. ✓
- Array di matching: stessi 3 dict del Python (hub kbd 05AC:029C up1/u6, hub knob 05AC:029C
  up12/u1, audio 0C76:1710 up12/u1), solo ordine diverso. ✓
- Edge detection consumer (`consumer.rs`): Vol± absolute, Mute/Play tap — coincide. ✓
- Nessun thread → `IOKitManager` con campo raw pointer `ctx_ptr` è usato solo sul main thread;
  non serve `Send`. ✓

## Stato: compila, 18 warning (tutti dead-code stub + naming C-style intenzionale).

---

# SECURITY AUDIT — GUI macOS (app con privilegi root)

L'app GUI gira con privilegi elevati (seize HID + inject keystroke), quindi è un
bersaglio per privilege escalation. Audit + fix applicati:

## CRITICO — Dylib hijacking (root code execution) → FIXATO
Prima la build linkava `libhagibis_hub_mapper.dylib` dinamicamente, caricandolo da
`@executable_path`. Essendo l'app eseguita come root, un attaccante con accesso in
scrittura a `.app/Contents/MacOS/` (il bundle stava in `~/dev`, user-writable) poteva
sostituire il dylib → esecuzione di codice arbitrario come root al successivo avvio.
**Fix**: static linking — si passa `libhagibis_hub_mapper.a` direttamente a swiftc
(non `-l`). Binario self-contained (7.9MB), nessun dylib hijackable. Verificato con
`otool -L` (nessuna dipendenza hagibis).

## ALTO — Shell command injection nell'elevazione → FIXATO
Prima l'elevazione usava `osascript "do shell script \"...\(execPath)...\" with
administrator privileges"` — il path veniva interpolato in una stringa shell eseguita
come root. Un `.app` in un path con metacaratteri (`$()`, backtick, `;`) permetteva
root command injection (l'escaping gestiva solo `"`).
**Fix**: `elevate.c` usa `AuthorizationExecuteWithPrivileges` passando il path come
ARGOMENTO diretto (mai in una shell). Nessun vettore di injection anche con path ostili.

## MEDIO — Keystroke injection via config → FIXATO
Il config TOML definisce combo iniettate dal processo root. Un config group/world-writable
permetterebbe a un utente locale non privilegiato di iniettare keystroke arbitrari
(= comandi) nella sessione privilegiata.
**Fix**: `manager::is_safe_permissions()` rifiuta config con permessi `0o022` (group/world
writable), ricadendo sui default built-in. I config creati sono `chmod 600`.

## MEDIO — Config path inconsistente come root → FIXATO
Come root `$HOME=/var/root`, quindi l'engine leggeva un config diverso da quello editato
dall'utente. **Fix**: `real_home()` risolve la home dell'utente reale (owner di
`/dev/console`) quando euid==0, mantenendo engine e config sincronizzati.

## Bug — Parsing JSON fragile (Swift) → FIXATO
`JSONSerialization` produce `NSNumber`; il cast `as? UInt8`/`as? [UInt8]` poteva fallire
silenziosamente → stato sempre 0. **Fix**: parsing via `Int` + `UInt8(truncatingIfNeeded:)`.

## Bug — "Application is no longer open" → FIXATO
Il parent non-root chiamava `exit(0)` senza mai avviare `NSApplication`, quindi
LaunchServices lo segnalava come crash. **Fix**: `LauncherDelegate` avvia un
`NSApplication` minimale, esegue l'elevazione in `applicationDidFinishLaunching`, poi
`NSApp.terminate` pulito. Combinato col static link (il child root non crasha più sul
caricamento dylib).

## RISCHIO RESIDUO — GUI come root su macOS moderno
`AuthorizationExecuteWithPrivileges` lancia il child come root. Su macOS recenti (11+) un
processo root potrebbe NON connettersi al WindowServer della sessione utente → la menu bar
potrebbe non comparire. Se così, l'architettura corretta è: GUI come utente + helper
privilegiato (SMJobBless/LaunchDaemon) per seize+inject via IPC. Da valutare dopo il test.

## Note pulizia automatica OS
Alla morte del processo (crash/kill), IOKit rilascia automaticamente il device seized →
l'hub torna normale senza intervento. Nessun handle leak persistente.

---

# BUNDLE macOS — Setup, troubleshooting e lezioni apprese

## Stato finale: FUNZIONANTE

Doppio click su `OverrideHub.app` da Finder → dialog admin macOS standard (AEWP) →
password → `●HH` nella menu bar → "Show Monitor" → overlay con label live e mapping.

## Setup completo

### 1. Entitlements (`OverrideHub.entitlements`)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.security.device.usb</key>
    <true/>
</dict>
</plist>
```

**NON** includere `com.apple.security.app-sandbox` — il sandbox blocca
`kIOHIDOptionsTypeSeizeDevice` (IOKit HID exclusive access).

`com.apple.security.device.usb` è l'unica chiave necessaria. Sufficiente per
sbloccare IOKit nel contesto AEWP (AuthorizationExecuteWithPrivileges) sul bundle.

### 2. Firma (`build.sh`)

```bash
xattr -cr "$APP_BUNDLE"
codesign --force --deep --sign - \
  --entitlements OverrideHub.entitlements \
  --options runtime \
  "$APP_BUNDLE"
```

- `--sign -`: firma ad-hoc (nessun certificato Developer ID)
- `--options runtime`: attiva Hardened Runtime (richiesto da macOS per consentire
  l'accesso IOKit a processi elevati via AEWP)
- `--deep`: firma ricorsivamente tutti i binari nel bundle
- `xattr -cr`: rimuove attributi estesi prima della firma (evita "resource fork
  not allowed")

### 3. Reset TCC (Transparency, Consent, and Control)

Dopo la prima firma, macOS può memorizzare lo stato TCC vecchio. Reset:

```bash
tccutil reset ListenEvent com.gabrielebaldassarre.override-hub
tccutil reset All com.gabrielebaldassarre.override-hub
```

### 4. Quarantine (se l'app è stata scaricata/spostata)

```bash
xattr -dr com.apple.quarantine OverrideHub.app
xattr -cr OverrideHub.app
```

## Troubleshooting cronologico

### Problema 1: "L'applicazione non è più aperta" (error -600 da LaunchServices)

**Causa**: `exit(0)` chiamato prima di `NSApplication.run()` → LaunchServices
interpreta come crash.

**Fix**: non applicabile. Il `exit(0)` avviene nel processo padre NON-root dopo
aver spawnato il child root via AEWP. Il child root esegue `NSApplication.run()`
normalmente. Il problema era più a valle (vedi sotto).

**Nota**: `open <path>` dà errore -600 su app con `LSUIElement=true`. Usare
`open -a OverrideHub.app` o doppio click da Finder.

### Problema 2: Gatekeeper blocca il lancio

**Sintomo**: `spctl --assess OverrideHub.app → rejected`. Nessun dialog,
nessun errore visibile. L'app semplicemente non parte.

**Causa**: firma ad-hoc non basta su macOS Sequoia/Tahoe.

**Fix**: tasto destro sull'app → "Apri" (bypass Gatekeeper una tantum).
Dopo il primo bypass, i lanci successivi funzionano.

In alternativa: `xattr -dr com.apple.quarantine OverrideHub.app`.

### Problema 3: Engine non parte — label vuote, mapping assente

**Sintomo**: l'app si avvia, menu bar funziona, ma le label sono vuote,
nessun mapping, il pulsante Start/Stop non fa nulla.

**Root cause**: `kIOReturnExclusiveAccess` (0xE00002E2). Il child lanciato
via AEWP non ha accesso IOKit sufficiente per `IOHIDManagerOpen(kSeizeDevice)`.
Il device è già aperto dal sistema operativo e AEWP non dà i permessi per
strapparlo.

**Fix**: entitlements con `com.apple.security.device.usb = true` +
`codesign --options runtime`. Questa combinazione sblocca IOKit nel
contesto AEWP.

**Verifica**: il log `~/Library/Logs/override-hub/hagibis.log` deve mostrare:
```
[INFO] seize: IOHIDManagerOpen OK (ret=0x00000000)
```

### Problema 4: ELEVATE AEWP funziona sul binario nudo ma non nel bundle

**Causa**: il binario nudo lanciato da Finder viene aperto in Terminal.app.
Terminal ha un contesto di sicurezza diverso (più permissivo) rispetto a
LaunchServices che lancia direttamente il bundle. AEWP nel contesto Terminal
eredita accesso IOKit completo; nel contesto LaunchServices no.

**Fix**: le entitlements + `--options runtime` risolvono per entrambi i
contesti.

## Lezioni apprese

1. **Mai fare `exit(0)` prima di `NSApplication.run()`** — LaunchServices
   interpreta come crash. Invece: LauncherDelegate + `NSApp.terminate(nil)`.

2. **Static linking è più sicuro del dynamic linking** — il dylib hijacking
   è un vettore di privilege escalation reale quando l'app gira come root.

3. **Entitlements sono obbligatori per IOKit in contesto AEWP** — senza
   `com.apple.security.device.usb` e `--options runtime`, IOKit HID non funziona.

4. **TCC reset dopo ogni cambio di firma/entitlements** — macOS cache
   aggressivamente lo stato dei permessi.

5. **`open <path>` vs `open -a <name>`** — con `LSUIElement=true`, solo
   `open -a` funziona da terminale. Doppio click da Finder sempre OK.

6. **Testare sempre con `pgrep` e il log** — i processi zombie IOKit
   (seize bloccante) sopravvivono a `kill -9` e richiedono lo scollegamento
   fisico dell'hub USB.

## Architettura bundle

```
OverrideHub.app/
├── Contents/
│   ├── Info.plist              # LSUIElement=true, CFBundleIdentifier, etc.
│   ├── MacOS/OverrideHub       # binario statico (7.9 MB)
│   ├── Resources/
│   │   ├── AppIcon.icns        # icona Finder/Dock
│   │   └── hub.png             # foto hub per overlay
│   └── _CodeSignature/         # firma ad-hoc con entitlements
├── OverrideHub.entitlements    # com.apple.security.device.usb = true
└── build.sh                    # cargo + swiftc + codesign + bundle
```
