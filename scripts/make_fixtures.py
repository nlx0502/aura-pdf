"""Génère les PDF de test d'Aura PDF (sans dépendance externe).

    python scripts/make_fixtures.py

Produit dans tests/fixtures/ :
  - Rapport Q1 2025.pdf : 14 pages, signets, liens internes, pièce jointe, métadonnées ;
  - Gros document.pdf   : 600 pages (performances, virtualisation) ;
  - Paysage.pdf         : pages de tailles et d'orientations mélangées ;
  - Données sensibles.pdf : IBAN, e-mails, téléphones (caviardage) ;
  - Plan avec calques.pdf : contenu optionnel (calques).
"""
import os
import zlib

OUT = os.path.join(os.path.dirname(__file__), "..", "tests", "fixtures")


def esc(s: str) -> bytes:
    b = s.encode("cp1252", errors="replace")
    return b.replace(b"\\", b"\\\\").replace(b"(", b"\\(").replace(b")", b"\\)")


def utf16(s: str) -> bytes:
    return b"<FEFF" + s.encode("utf-16-be").hex().upper().encode() + b">"


class Pdf:
    def __init__(self):
        self.objs = []

    def add(self, body: bytes) -> int:
        self.objs.append(body)
        return len(self.objs)

    def set(self, num: int, body: bytes):
        self.objs[num - 1] = body

    def reserve(self) -> int:
        return self.add(b"null")

    def save(self, path: str, root: int, info: int):
        out = bytearray(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n")
        offsets = []
        for i, body in enumerate(self.objs, 1):
            offsets.append(len(out))
            out += f"{i} 0 obj\n".encode() + body + b"\nendobj\n"
        xref = len(out)
        out += f"xref\n0 {len(self.objs) + 1}\n0000000000 65535 f \n".encode()
        for o in offsets:
            out += f"{o:010d} 00000 n \n".encode()
        out += f"trailer\n<< /Size {len(self.objs) + 1} /Root {root} 0 R /Info {info} 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode()
        with open(path, "wb") as f:
            f.write(out)


def stream(data: bytes, extra: bytes = b"") -> bytes:
    z = zlib.compress(data)
    return b"<< /Length %d /Filter /FlateDecode %s>>\nstream\n" % (len(z), extra) + z + b"\nendstream"


def wrap(text: str, width: int):
    words, line, out = text.split(), "", []
    for w in words:
        if len(line) + len(w) + 1 > width:
            out.append(line)
            line = w
        else:
            line = (line + " " + w).strip()
    if line:
        out.append(line)
    return out


PARAS = [
    "L’exercice entamé au premier trimestre 2025 confirme une accélération marquée de l’adoption des architectures documentaires légères. "
    "La transition vers des outils à latence quasi nulle a réduit de 42 % le cycle de révision contractuelle, plaçant l’équipe au premier rang des environnements de nouvelle génération.",
    "L’intégration de modèles contextuels permet désormais l’audit sémantique automatique des annexes financières, sans exfiltration de métadonnées sensibles et avec un traitement intégralement local.",
    "Les annexes techniques restent la principale source de révisions tardives ; un circuit de relecture dédié est proposé pour le prochain trimestre.",
]
SECTIONS = ["Résumé exécutif", "Métriques clés", "Latence de traitement", "Conformité", "Prochaines étapes", "Annexes"]


def page_content(n: int, total: int, title: str, w: float, h: float, heading: str | None) -> bytes:
    ops = []
    ops.append(b"0.42 0.42 0.39 rg BT /F1 9 Tf 56 %.1f Td (Aura Global Dynamics) Tj ET" % (h - 50))
    ops.append(b"BT /F1 9 Tf %.1f %.1f Td (Page %d sur %d) Tj ET" % (w - 130, h - 50, n, total))
    y = h - 110
    ops.append(b"0.43 0.42 0.96 rg BT /F2 10 Tf 56 %.1f Td (RAPPORT TRIMESTRIEL) Tj ET" % y)
    y -= 34
    ops.append(b"0.1 0.1 0.12 rg BT /F2 24 Tf 56 %.1f Td (" % y + esc(title) + b") Tj ET")
    y -= 40
    if heading:
        ops.append(b"BT /F2 15 Tf 56 %.1f Td (" % y + esc(heading) + b") Tj ET")
        y -= 28
    for p in PARAS:
        for line in wrap(p, 92):
            ops.append(b"0.15 0.15 0.17 rg BT /F1 11 Tf 56 %.1f Td (" % y + esc(line) + b") Tj ET")
            y -= 17
        y -= 12
    # Graphique en barres
    bars = [38, 52, 46, 70, 64, 88]
    base = y - 120
    for i, b in enumerate(bars):
        c = b"0.43 0.42 0.96" if i == 5 else b"0.85 0.84 0.8"
        ops.append(c + b" rg %.1f %.1f 40 %.1f re f" % (70 + i * 60, base, b * 1.2))
    ops.append(b"0.42 0.42 0.39 rg BT /F1 9 Tf 56 %.1f Td (" % (base - 20) + esc("Volume de documents traités par mois") + b") Tj ET")
    y = base - 60
    if n == 2:
        ops.append(b"0.43 0.42 0.96 rg BT /F1 11 Tf 56 %.1f Td (" % y + esc("Voir la section Conformité, page 7 (lien interne).") + b") Tj ET")
    return b"\n".join(ops)


def build_report(path: str, pages: int = 14, title: str = "Q1 2025 — Stratégie & innovation", big: bool = False):
    pdf = Pdf()
    catalog = pdf.reserve()
    pages_obj = pdf.reserve()
    f1 = pdf.add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>")
    f2 = pdf.add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>")
    page_ids = []
    w, h = 595.28, 841.89
    for n in range(1, pages + 1):
        heading = SECTIONS[(n - 2) % len(SECTIONS)] if n > 1 and not big else (f"Chapitre {n}" if big else None)
        content = pdf.add(stream(page_content(n, pages, title, w, h, heading)))
        pid = pdf.reserve()
        page_ids.append((pid, content))
    # Lien interne page 2 → page 7
    for i, (pid, content) in enumerate(page_ids):
        annots = b""
        if i == 1 and len(page_ids) >= 7:
            target = page_ids[6][0]
            link = pdf.add(b"<< /Type /Annot /Subtype /Link /Rect [56 %.1f 420 %.1f] /Border [0 0 0] /Dest [%d 0 R /XYZ 0 842 0] >>" % (95, 115, target))
            annots = b" /Annots [%d 0 R]" % link
        pdf.set(pid, b"<< /Type /Page /Parent %d 0 R /MediaBox [0 0 %.2f %.2f] /Contents %d 0 R /Resources << /Font << /F1 %d 0 R /F2 %d 0 R >> >>%s >>" % (pages_obj, w, h, content, f1, f2, annots))
    kids = b" ".join(b"%d 0 R" % pid for pid, _ in page_ids)
    pdf.set(pages_obj, b"<< /Type /Pages /Kids [%s] /Count %d >>" % (kids, len(page_ids)))

    # Signets (table des matières)
    outline_root = pdf.reserve()
    entries = [("Rapport trimestriel Q1 2025", 0, 0)] + [(s, 1, min(len(page_ids) - 1, 1 + i)) for i, s in enumerate(SECTIONS)]
    if big:
        entries = [(f"Chapitre {i + 1}", 0, i * 50) for i in range(len(page_ids) // 50)]
    ids = [pdf.reserve() for _ in entries]
    for k, (t, depth, p) in enumerate(entries):
        prev = b" /Prev %d 0 R" % ids[k - 1] if k > 0 else b""
        nxt = b" /Next %d 0 R" % ids[k + 1] if k < len(entries) - 1 else b""
        pdf.set(ids[k], b"<< /Title " + utf16(t) + b" /Parent %d 0 R /Dest [%d 0 R /Fit]%s%s >>" % (outline_root, page_ids[p][0], prev, nxt))
    pdf.set(outline_root, b"<< /Type /Outlines /First %d 0 R /Last %d 0 R /Count %d >>" % (ids[0], ids[-1], len(ids)))

    # Pièce jointe
    att_data = "Mois;Volume\nJanvier;380\nFévrier;520\nMars;460\n".encode("utf-8")
    ef = pdf.add(stream(att_data, b"/Type /EmbeddedFile /Params << /Size %d >> " % len(att_data)))
    fs = pdf.add(b"<< /Type /Filespec /F (Annexe A - volumes.csv) /UF " + utf16("Annexe A — volumes.csv") + b" /EF << /F %d 0 R >> >>" % ef)
    names = pdf.add(b"<< /EmbeddedFiles << /Names [" + utf16("Annexe A — volumes.csv") + b" %d 0 R] >> >>" % fs)

    pdf.set(catalog, b"<< /Type /Catalog /Pages %d 0 R /Outlines %d 0 R /PageMode /UseOutlines /Names %d 0 R >>" % (pages_obj, outline_root, names))
    info = pdf.add(
        b"<< /Title " + utf16(title) + b" /Author " + utf16("Division ingénierie") + b" /Producer (Aura PDF fixtures) /CreationDate (D:20250114093000+01'00') /ModDate (D:20250402170000+02'00') >>"
    )
    pdf.save(path, catalog, info)


def build_mixed(path: str):
    pdf = Pdf()
    catalog = pdf.reserve()
    pages_obj = pdf.reserve()
    f1 = pdf.add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>")
    sizes = [(595.28, 841.89, 0), (841.89, 595.28, 0), (595.28, 841.89, 90), (419.53, 595.28, 0)]
    pids = []
    for i, (w, h, rot) in enumerate(sizes, 1):
        c = pdf.add(stream(b"BT /F1 28 Tf 60 %.1f Td (" % (h - 120) + esc(f"Page {i} — {int(w)}×{int(h)} pt, rotation {rot}°") + b") Tj ET"))
        pids.append(pdf.add(b"<< /Type /Page /Parent %d 0 R /MediaBox [0 0 %.2f %.2f] /Rotate %d /Contents %d 0 R /Resources << /Font << /F1 %d 0 R >> >> >>" % (pages_obj, w, h, rot, c, f1)))
    pdf.set(pages_obj, b"<< /Type /Pages /Kids [%s] /Count %d >>" % (b" ".join(b"%d 0 R" % p for p in pids), len(pids)))
    pdf.set(catalog, b"<< /Type /Catalog /Pages %d 0 R >>" % pages_obj)
    info = pdf.add(b"<< /Title " + utf16("Pages mélangées") + b" >>")
    pdf.save(path, catalog, info)


PAD = bytes.fromhex("28BF4E5E4E758A4164004E56FFFA01082E2E00B6D0683E802F0CA9FE6453697A")


def rc4(key: bytes, data: bytes) -> bytes:
    s = list(range(256))
    j = 0
    for i in range(256):
        j = (j + s[i] + key[i % len(key)]) % 256
        s[i], s[j] = s[j], s[i]
    out, i, j = bytearray(), 0, 0
    for b in data:
        i = (i + 1) % 256
        j = (j + s[i]) % 256
        s[i], s[j] = s[j], s[i]
        out.append(b ^ s[(s[i] + s[j]) % 256])
    return bytes(out)


def build_encrypted(path: str, user_pw: str = "aura", owner_pw: str = "proprietaire"):
    """PDF chiffré (gestionnaire standard, révision 3, RC4 128 bits)."""
    import hashlib

    md5 = lambda b: hashlib.md5(b).digest()
    pad = lambda pw: (pw.encode("latin-1") + PAD)[:32]
    file_id = md5(b"aura-pdf-fixture")
    p = -3904
    # Clé O
    k = md5(pad(owner_pw))
    for _ in range(50):
        k = md5(k)
    o = rc4(k, pad(user_pw))
    for i in range(1, 20):
        o = rc4(bytes(b ^ i for b in k), o)
    # Clé de chiffrement
    key = md5(pad(user_pw) + o + p.to_bytes(4, "little", signed=True) + file_id)
    for _ in range(50):
        key = md5(key[:16])
    key = key[:16]
    # Clé U
    u = rc4(key, md5(PAD + file_id))
    for i in range(1, 20):
        u = rc4(bytes(b ^ i for b in key), u)
    u += b"\x00" * 16

    def obj_key(num: int) -> bytes:
        return md5(key + num.to_bytes(3, "little") + (0).to_bytes(2, "little"))[:16]

    pdf = Pdf()
    catalog = pdf.reserve()
    pages_obj = pdf.reserve()
    f1 = pdf.add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>")
    content_num = len(pdf.objs) + 1
    raw = b"BT /F1 26 Tf 60 720 Td (" + esc("Document confidentiel") + b") Tj ET BT /F1 13 Tf 60 690 Td (" + esc("Ouvert avec le mot de passe « aura ».") + b") Tj ET"
    enc = rc4(obj_key(content_num), raw)
    pdf.add(b"<< /Length %d >>\nstream\n" % len(enc) + enc + b"\nendstream")
    page = pdf.add(b"<< /Type /Page /Parent %d 0 R /MediaBox [0 0 595.28 841.89] /Contents %d 0 R /Resources << /Font << /F1 %d 0 R >> >> >>" % (pages_obj, content_num, f1))
    pdf.set(pages_obj, b"<< /Type /Pages /Kids [%d 0 R] /Count 1 >>" % page)
    pdf.set(catalog, b"<< /Type /Catalog /Pages %d 0 R >>" % pages_obj)
    encrypt = pdf.add(b"<< /Filter /Standard /V 2 /R 3 /Length 128 /P %d /O <%s> /U <%s> >>" % (p, o.hex().encode(), u.hex().encode()))
    info = pdf.add(b"<< >>")
    pdf.save(path, catalog, info)
    # Ajouter /Encrypt et /ID à la remorque.
    with open(path, "rb") as f:
        data = f.read()
    data = data.replace(b"/Info %d 0 R >>" % info, b"/Info %d 0 R /Encrypt %d 0 R /ID [<%s> <%s>] >>" % (info, encrypt, file_id.hex().encode(), file_id.hex().encode()))
    with open(path, "wb") as f:
        f.write(data)


def build_sensitive(path: str):
    """Dossier contenant des données personnelles (caviardage) : IBAN, e-mails,
    téléphones, dans du texte simple, un tableau TJ crénelé et un XObject de formulaire."""
    pdf = Pdf()
    catalog = pdf.reserve()
    pages_obj = pdf.reserve()
    f1 = pdf.add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>")
    f2 = pdf.add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>")
    w, h = 595.28, 841.89

    def line(y, text, size=11, font=b"/F1"):
        return b"BT %s %d Tf 56 %.1f Td (" % (font, size, y) + esc(text) + b") Tj ET"

    # Page 1 : virement et contact.
    p1 = [
        b"0.42 0.42 0.39 rg BT /F1 9 Tf 56 %.1f Td (Aura Global Dynamics) Tj ET" % (h - 50),
        b"0.1 0.1 0.12 rg",
        line(h - 120, "Dossier d’audit Q4", 22, b"/F2"),
        line(h - 170, "Le règlement des honoraires est à effectuer par virement sur le compte"),
        line(h - 187, "FR76 3000 6000 0112 3456 7890 189 avant le 30 novembre 2025."),
        # Ligne en TJ avec crénage : le caviardage doit garder la position du reste.
        b"BT /F1 11 Tf 56 %.1f Td [(Contact : ) -15 (jeanne.martin@exemple.fr) 0 ( \x97 t\xe9l. 06 12 34 56 78, du lundi au vendredi.)] TJ ET" % (h - 204),
        line(h - 240, "Les pièces justificatives sont conservées dans l’espace d’audit sécurisé."),
        line(h - 257, "Aucune donnée ne quitte le poste de travail pendant la revue."),
    ]
    # Page 2 : image et formulaire XObject contenant un e-mail.
    img_w, img_h = 120, 60
    pixels = bytearray()
    for y in range(img_h):
        for x in range(img_w):
            pixels += bytes((110 + x, 106 + y, 246))
    img = pdf.add(stream(bytes(pixels), b"/Type /XObject /Subtype /Image /Width %d /Height %d /ColorSpace /DeviceRGB /BitsPerComponent 8 " % (img_w, img_h)))
    form_content = b"BT /F1 11 Tf 0 0 Td (" + esc("Référent : paul.durand@audit-exemple.fr") + b") Tj ET"
    form = pdf.add(stream(form_content, b"/Type /XObject /Subtype /Form /BBox [0 -4 400 14] /Resources << /Font << /F1 %d 0 R >> >> " % f1))
    p2 = [
        b"0.1 0.1 0.12 rg",
        line(h - 120, "Annexe — interlocuteurs", 18, b"/F2"),
        b"q 1 0 0 1 56 %.1f cm /Fm1 Do Q" % (h - 170),
        line(h - 190, "Ligne directe : +33 (0)1 42 68 53 00 (standard)."),
        b"q 240 0 0 120 56 %.1f cm /Im1 Do Q" % (h - 340),
        line(h - 360, "Schéma du réseau d’audit."),
    ]
    p3 = [
        b"0.1 0.1 0.12 rg",
        line(h - 120, "Synthèse des contrôles", 18, b"/F2"),
        line(h - 160, "Les contrôles d’accès ont été revus sur l’ensemble du périmètre."),
        line(h - 177, "Deux écarts mineurs restent ouverts et font l’objet d’un plan d’action."),
    ]
    kids = []
    for ops in (p1, p2, p3):
        c = pdf.add(stream(b"\n".join(ops)))
        kids.append(pdf.add(b"<< /Type /Page /Parent %d 0 R /MediaBox [0 0 %.2f %.2f] /Contents %d 0 R /Resources << /Font << /F1 %d 0 R /F2 %d 0 R >> /XObject << /Im1 %d 0 R /Fm1 %d 0 R >> >> >>" % (pages_obj, w, h, c, f1, f2, img, form)))
    pdf.set(pages_obj, b"<< /Type /Pages /Kids [%s] /Count %d >>" % (b" ".join(b"%d 0 R" % k for k in kids), len(kids)))
    pdf.set(catalog, b"<< /Type /Catalog /Pages %d 0 R >>" % pages_obj)
    info = pdf.add(b"<< /Title " + utf16("Dossier d’audit Q4") + b" /Author " + utf16("Jeanne Martin") + b" /Producer (Aura PDF fixtures) >>")
    pdf.save(path, catalog, info)


def build_layers(path: str):
    """Plan avec deux calques (contenu optionnel), dont un masqué par défaut."""
    pdf = Pdf()
    catalog = pdf.reserve()
    pages_obj = pdf.reserve()
    f1 = pdf.add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>")
    cotes = pdf.add(b"<< /Type /OCG /Name " + utf16("Cotes") + b" >>")
    mobilier = pdf.add(b"<< /Type /OCG /Name " + utf16("Mobilier") + b" >>")
    w, h = 841.89, 595.28
    ops = [
        b"0.1 0.1 0.12 rg BT /F1 20 Tf 50 540 Td (" + esc("Plan du rez-de-chaussée") + b") Tj ET",
        b"0.2 0.2 0.25 RG 3 w 80 80 680 400 re S 420 80 m 420 480 l S",
        b"/OC /Cotes BDC 0.85 0.27 0.24 RG 0.85 0.27 0.24 rg 4 w 80 60 m 760 60 l S BT /F1 12 Tf 400 44 Td (" + esc("24,0 m") + b") Tj ET EMC",
        b"/OC /Mobilier BDC 0.31 0.8 0.57 rg 120 120 120 60 re f 520 300 160 90 re f BT /F1 11 Tf 130 190 Td (Bureau) Tj ET EMC",
    ]
    c = pdf.add(stream(b"\n".join(ops)))
    page = pdf.add(b"<< /Type /Page /Parent %d 0 R /MediaBox [0 0 %.2f %.2f] /Contents %d 0 R /Resources << /Font << /F1 %d 0 R >> /Properties << /Cotes %d 0 R /Mobilier %d 0 R >> >> >>" % (pages_obj, w, h, c, f1, cotes, mobilier))
    pdf.set(pages_obj, b"<< /Type /Pages /Kids [%d 0 R] /Count 1 >>" % page)
    pdf.set(catalog, b"<< /Type /Catalog /Pages %d 0 R /OCProperties << /OCGs [%d 0 R %d 0 R] /D << /Order [%d 0 R %d 0 R] /OFF [%d 0 R] >> >> >>" % (pages_obj, cotes, mobilier, cotes, mobilier, mobilier))
    info = pdf.add(b"<< /Title " + utf16("Plan avec calques") + b" >>")
    pdf.save(path, catalog, info)


if __name__ == "__main__":
    os.makedirs(OUT, exist_ok=True)
    build_report(os.path.join(OUT, "Rapport Q1 2025.pdf"))
    build_report(os.path.join(OUT, "Gros document.pdf"), pages=600, title="Référentiel technique (600 pages)", big=True)
    build_mixed(os.path.join(OUT, "Paysage.pdf"))
    build_encrypted(os.path.join(OUT, "Protégé (aura).pdf"))
    build_sensitive(os.path.join(OUT, "Données sensibles.pdf"))
    build_layers(os.path.join(OUT, "Plan avec calques.pdf"))
    with open(os.path.join(OUT, "Endommagé.pdf"), "wb") as f:
        f.write(b"%PDF-1.7\nceci n'est pas un PDF valide\n")
    print("PDF de test générés dans", os.path.abspath(OUT))
