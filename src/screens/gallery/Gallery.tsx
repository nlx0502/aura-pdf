import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Accordion,
  Button,
  Checkbox,
  Chip,
  IconButton,
  Icons,
  Menu,
  MenuItem,
  Modal,
  ModalHeader,
  PageThumb,
  ProgressBar,
  Segmented,
  Slider,
  StepList,
  Stepper,
  Switch,
  TextField,
  toast,
  type StepState,
} from "../../components";
import { useApp } from "../../stores/app";
import { Logo } from "../../shell/AppBar";
import { WindowControls } from "../../shell/WindowControls";

/** Vitrine de la bibliothèque de composants (accessible via Ctrl+K). */
export function Gallery() {
  const { t } = useTranslation();
  const goHome = useApp((s) => s.goHome);
  const [seg, setSeg] = useState<"continu" | "page" | "double">("continu");
  const [sw, setSw] = useState(true);
  const [chip, setChip] = useState("all");
  const [cb, setCb] = useState(true);
  const [slider, setSlider] = useState(75);
  const [steps, setSteps] = useState(2);
  const [menu, setMenu] = useState(false);
  const [modal, setModal] = useState(false);
  const [run, setRun] = useState(-1);
  const menuRef = useRef<HTMLButtonElement>(null);

  const stepStates = (i: number): StepState => (run < 0 ? "pending" : i < run ? "done" : i === run ? "active" : "pending");
  const startSteps = () => {
    setRun(0);
    [1, 2, 3, 4].forEach((k) => window.setTimeout(() => setRun(k), k * 900));
  };

  const Section = ({ title, children }: { title: string; children: React.ReactNode }) => (
    <section className="gal__sec">
      <h2 className="title">{title}</h2>
      <div className="gal__row">{children}</div>
    </section>
  );

  return (
    <div className="home">
      <header className="home__bar" data-tauri-drag-region>
        <Logo width={232} />
        <div style={{ flex: 1 }} data-tauri-drag-region />
        <Button variant="ghost" onClick={goHome}>
          {t("app.homeLink")}
        </Button>
        <WindowControls />
      </header>
      <div className="workspace">
        <main className="panel home__main">
          <div className="home__inner">
            <div className="home__hello">
              <h1>{t("gallery.title")}</h1>
              <span className="muted" style={{ fontSize: 14 }}>
                {t("gallery.subtitle")}
              </span>
            </div>
            <Section title="Boutons">
              <Button variant="primary" icon={<Icons.export />}>
                Exporter
              </Button>
              <Button>Secondaire</Button>
              <Button variant="ghost">Fantôme</Button>
              <Button variant="danger" icon={<Icons.trash />}>
                Supprimer
              </Button>
              <Button variant="primary" kbd="Ctrl+O">
                Ouvrir un fichier
              </Button>
              <IconButton label="Imprimer" shortcut="Ctrl+P" icon={<Icons.print />} />
              <IconButton label="Mode sombre" on icon={<Icons.moon />} />
              <Button disabled>Désactivé</Button>
            </Section>
            <Section title="Contrôle segmenté, interrupteur, pastilles">
              <Segmented
                label="Mode d’affichage"
                value={seg}
                onChange={setSeg}
                itemWidth={92}
                options={[
                  { value: "continu", label: "Continu" },
                  { value: "page", label: "Page" },
                  { value: "double", label: "Double" },
                ]}
              />
              <Switch label="Recto verso" checked={sw} onChange={setSw} />
              {["all", "signed", "form", "scan"].map((c) => (
                <Chip key={c} pressed={chip === c} onClick={() => setChip(c)}>
                  {t(`home.filters.${c}`)}
                </Chip>
              ))}
            </Section>
            <Section title="Champs, curseur, pas à pas, case à cocher">
              <div style={{ width: 280 }}>
                <TextField icon={<Icons.search />} pill placeholder="Rechercher dans le document" />
              </div>
              <div style={{ width: 220 }}>
                <TextField placeholder="ex. 1-5, 8, 11-13" />
              </div>
              <div style={{ width: 240 }}>
                <Slider label="Échelle" value={slider} min={30} max={140} step={5} onChange={setSlider} format={(v) => `${v} %`} />
              </div>
              <Stepper value={steps} onChange={setSteps} decLabel="Moins" incLabel="Plus" />
              <Checkbox checked={cb} onChange={setCb}>
                Assembler les copies
              </Checkbox>
            </Section>
            <Section title="Menu, dialogue, notifications">
              <Button ref={menuRef} icon={<Icons.tools />} onClick={() => setMenu(!menu)}>
                Outils
              </Button>
              <Menu anchor={menuRef} open={menu} onClose={() => setMenu(false)} label="Outils">
                <MenuItem icon={<Icons.merge />} tileBg="var(--accent-soft)" tileFg="var(--accent-text)" label="Fusionner et diviser" desc="Combiner plusieurs PDF ou en découper un" onSelect={() => {}} />
                <MenuItem icon={<Icons.compress />} tileBg="var(--success-soft)" tileFg="var(--success)" label="Compresser" desc="Réduire la taille du fichier" delay={35} onSelect={() => {}} />
              </Menu>
              <Button onClick={() => setModal(true)}>Ouvrir un dialogue</Button>
              <Button onClick={() => toast.success("Enregistré")}>Notification</Button>
              <Button onClick={() => toast.show({ kind: "loading", message: "Ouverture de Rapport.pdf…", duration: 2500 })}>Chargement</Button>
              <Button onClick={() => toast.error("Mot de passe incorrect. Vérifiez la saisie et réessayez.")}>Erreur</Button>
            </Section>
            <Section title="Progression et étapes">
              <div style={{ width: 280, display: "flex", flexDirection: "column", gap: 10 }}>
                <ProgressBar value={64} />
                <ProgressBar />
                <ProgressBar value={100} tone="success" thin />
              </div>
              <div style={{ width: 320 }}>
                <StepList
                  steps={[
                    { label: "Certificat", state: stepStates(0) },
                    { label: "Empreinte du document", state: stepStates(1) },
                    { label: "Horodatage", state: stepStates(2) },
                    { label: "Intégration de la signature", state: stepStates(3) },
                  ]}
                />
                <Button size="sm" onClick={startSteps}>
                  Lancer
                </Button>
              </div>
            </Section>
            <Section title="Miniature de page, accordéon">
              <div style={{ width: 110 }}>
                <PageThumb number={2} current aspect={1.414} label="Page 2" />
              </div>
              <div style={{ width: 110 }}>
                <PageThumb number={3} aspect={1.414} label="Page 3" />
              </div>
              <div style={{ width: 340 }}>
                <Accordion title="Images" icon={<Icons.image />} defaultOpen>
                  <Slider label="Qualité JPEG" value={slider} min={30} max={100} onChange={setSlider} format={(v) => `${v} %`} />
                </Accordion>
                <Accordion title="Polices" icon={<Icons.fileText />}>
                  <span className="muted">Sous-ensembles de polices</span>
                </Accordion>
              </div>
            </Section>
          </div>
        </main>
      </div>
      <Modal open={modal} onClose={() => setModal(false)} label="Dialogue">
        <ModalHeader icon={<Icons.info />} title="Dialogue modal" subtitle="Fond flouté et apparition « pop »" onClose={() => setModal(false)} />
        <div className="dialog__body">
          <p className="muted">Échap ferme le dialogue ; le focus reste piégé à l’intérieur.</p>
        </div>
        <div className="dialog__foot">
          <Button variant="ghost" onClick={() => setModal(false)}>
            Annuler
          </Button>
          <Button variant="primary" onClick={() => setModal(false)}>
            Valider
          </Button>
        </div>
      </Modal>
    </div>
  );
}
