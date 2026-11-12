# Update-Metadaten signieren

Der Ed25519-Schlüssel ist unabhängig von Windows-/Apple-Zertifikaten. Der private
Schlüssel gehört dauerhaft in das Repository-Actions-Secret
`UPDATE_SIGNING_KEY_B64` (Base64 eines PKCS#8-PEM-Schlüssels). Nur der rohe öffentliche
32-Byte-Schlüssel steht Base64-kodiert in `assets/update-public-key.base64`.
Eine leere Datei sperrt Update-Installation und Release-Signierung ausdrücklich.
V1.5.0 darf damit nicht veröffentlicht werden.

Vor der ersten Schlüsselerzeugung muss authentifizierter Zugriff zum Schreiben
und Bestätigen des dauerhaften Actions-Secrets funktionieren. Schlüssel nicht in
Git, Chat, Shell-Argumente, Artefakte oder Protokolle schreiben. Temporäre Dateien
brauchen einen privaten Ordner und 0600-Rechte. Erst nach bestätigter Speicherung
des Secrets darf die temporäre private Kopie entfernt werden. Ein Actions-Secret
kann später nicht ausgelesen werden; für Schlüsselwechsel bleibt die bisherige
Signiermöglichkeit nötig oder eine manuelle Migration erforderlich.

Der Release-Job signiert die exakten Bytes von `update-metadata.json` und prüft
vorher Tag, Version, alle 16 Paketnamen und den öffentlichen Schlüssel des Secrets.
Die Metadaten binden Version, Paketname, Länge und SHA-256. `update-metadata.sig`
enthält die Base64-kodierte Ed25519-Signatur. Die App prüft Signatur und eine strikt
neuere Version vor dem Download, erneut vor der Übergabe und SHA-256 zusätzlich
im Installationshelfer nach dem Ende des alten Prozesses. Kein Rückfall auf
unsignierte Prüfsummen. Alte Clients bis 1.4.9 können den Vertrauensschlüssel erst
mit einem manuell vertrauten Upgrade erhalten; ihre alte Prüfsummenprüfung kann
nachträglich nicht gehärtet werden.

Die Signierdatei ist ein Packaging-Schritt, kein Test. Es werden keine Tests oder
App-/Installationstarts ausgeführt. Alle sechs nativen Builds und sämtliche
Installer müssen erfolgreich sein. Erst danach darf der Release veröffentlicht
werden. Repository-Zugriff allein verleiht kein Vertrauen für Update-Pakete;
Schlüsselwechsel erfordert eine bereits vertrauenswürdige App-Version.
