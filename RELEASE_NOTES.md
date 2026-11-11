# EditorTool V1.4.1

## Einstellungen und Bedienung

- **Einstellungen** steht als ausgeschriebener Menüpunkt direkt neben **Bearbeiten**. Das zusätzliche Einstellungen-Icon in der Werkzeugleiste entfällt.
- **Allgemein** und **Clients und Sync** öffnen als normale Tabs in der bestehenden Datei-/Tab-Liste. Einstellungen bleiben damit im Editor statt in einem separaten Fenster. Die Seiten lassen sich schliessen und beim nächsten Start wiederherstellen. Kleine Fenster erhalten eine scrollbare Einstellungsseite.
- Client-Felder sind sichtbar beschriftet: **Client-Name**, **IP-Adresse**, **Port**, ausserdem **Name dieses Geräts** und **Lokaler Port**. Die Client-Liste besitzt eigene Spaltenüberschriften für Name, Adresse und Verbindungsstatus.
- Geräteverbindungen, Dateiempfang und Sync-Freigaben verwenden eigene Dialoge mit der Gestaltung der App, lesbaren Details und eindeutigen Aktionen. Schliessen oder Escape erteilt keine Freigabe.
- Der Textcursor blinkt im fokussierten Textfeld. Eingaben setzen die sichtbare Phase zurück; ausgeblendete und unfokussierte Textfelder benötigen keinen laufenden Blink-Timer.
- **Text-Zoom von 50 bis 300 %** über **Ansicht → Zoom**, **Strg/Cmd + Plus/Minus**, **Strg/Cmd + 0** und **Strg/Cmd + Mausrad**. Der aktuelle Zoom steht in der Statusleiste und wird im Sitzungscache je Textansicht gespeichert. Zoom verändert weder Dateiinhalte noch übertragene Bildgrössen.
- Textblöcke vor eingebetteten Bildern verwenden die tatsächliche Schrift-Zeilenhöhe statt einer Schätzung. Das behebt den mit der Textlänge wachsenden Leerraum vor Bildern unter macOS und berücksichtigt auch Zoom und Syntaxfarben.

## Senden und Sync

- **Sync beenden** wird den anderen Teilnehmern für genau dieses Dokument mitgeteilt. Sie entfernen den betreffenden Client aus der aktiven Dokument-Synchronisierung und erhalten einen Hinweis. Bei einer Gruppe bleiben die übrigen Clients verbunden; bei zwei Teilnehmern endet die Dokument-Synchronisierung auf beiden Seiten. Inhalte und lokale Dateien bleiben erhalten.
- Die Abmeldung wird lokal vorgemerkt und nachgeholt, falls ein Teilnehmer gerade offline ist. Bereits ausgetretene Teilnehmer werden bei weiteren Abgleichen ausgelassen. Andere Dokumente und die allgemeine Geräteverbindung bleiben erhalten.

## Hybrider Post-Quanten-Schutz

- Senden und Sync erfordern **TLS 1.3**, **AES-256-GCM** und den hybriden Schlüsselaustausch **X25519 + ML-KEM-768** (`X25519MLKEM768`). Die Implementierung stammt aus Rustls und AWS-LC; es gibt kein eigenes kryptografisches Protokoll und keinen stillen Rückfall auf rein klassischen Schlüsselaustausch.
- Die Einstellungen zeigen Transportverschlüsselung, hybriden Schlüsselaustausch und Geräteidentität getrennt an. Die Geräteidentität bleibt mit klassischen Signaturen und bestätigten SHA-256-Fingerabdrücken abgesichert; sie ist keine Post-Quanten-Signatur. Der neue Schutz betrifft den Schlüsselaustausch der übertragenen Daten.
- **Alle beteiligten Geräte müssen auf V1.4.1 oder neuer aktualisiert werden.** Die frühere reine Ring-Konfiguration aus V1.4 ist mit dem verpflichtenden hybriden Austausch nicht kompatibel. Vorhandene Geräteidentitäten, Freigaben, Dokumentzuordnungen und lokale Daten bleiben erhalten. Ein nicht erreichbarer oder inkompatibler Client erscheint als **Not connected**.
- Weiterhin nur interne IPv4-/IPv6-Netze und geroutete Netze/VPN. Kein Internet-Vermittlungsdienst, keine Cloud und keine automatische Portweiterleitung. TCP **51473** bleibt der änderbare Standard-Port. Der GitHub-Updater arbeitet unabhängig davon über HTTPS.

Technischer Hintergrund: [Rustls Standardverfahren](https://rustls.dev/docs/rustls/manual/_05_defaults/index.html), [AWS-LC Plattformunterstützung](https://aws.github.io/aws-lc-rs/platform_support.html).

## Pakete und Hinweise

Windows Setup-EXE und portables ZIP; Linux Flatpak, RPM, DEB und portables TAR.GZ; macOS DMG und App-ZIP. Jeweils **x64 und ARM64**, dazu `SHA256SUMS.txt`.

Alle Funktionen aus V1.4 sind enthalten. Auf Wunsch **keine automatisierten, interaktiven oder Installationstests**. Pakete ohne Entwicklerzertifikate; macOS mit Ad-hoc-Signatur ohne Notarisierung. Prüfsummen ersetzen keine Herausgebersignatur. Projektlizenz: **The Unlicense**; Drittanbieter behalten ihre Lizenzen.
