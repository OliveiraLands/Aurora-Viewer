# Plano de Reestilização Mobile (Celular e Tablet) — OliViewer

Este documento define a arquitetura, os padrões de UX/UI estilo jogos mobile modernos e a **ordem de prioridades** para transformar todas as janelas, barras, controles e fluxos de entrada do viewer em uma interface 100% responsiva e otimizada para toque (*touch-first*) em celulares (retrato e paisagem) e tablets.

---

## 1. Diagnóstico do Estado Atual

O repositório já possui uma base inicial sólida na branch `feat/aurora-android`:
- **`crates/aurora-viewer/src/touch.rs`**: define `FormFactor` (`Compact < 600dp`, `Medium 600..840dp`, `Expanded >= 840dp`), `MIN_TOUCH_TARGET_DP = 48.0`, `apply_touch_style()` e gestos de câmera `TouchNav` (1 dedo órbita/tap/long-press, 2 dedos pinça/pan).
- **`crates/aurora-viewer/src/android.rs` + `AuroraActivity.java`**: modo imersivo fullscreen, corte de câmera (*display cutout*), ponte JNI para gamepad e interceptação do botão Voltar (`nativeOnBackPressed`).
- **`crates/aurora-viewer/src/ui/bars/dock.rs`**: dock inferior com rolagem horizontal parcial no Android.
- **`crates/aurora-viewer/src/ui/widgets.rs`**: quase todas as janelas do viewer passam pelo componente central `Floater` e `Modal`.

### Principais lacunas atuais no mobile
1. **Janelas flutuantes estilo desktop (`Floater`)**: abrem com coordenadas/tamanhos fixos (ex.: `600x720`), sobrepõem-se umas às outras e possuem barras de título e alças de redimensionamento pequenas demais para o dedo.
2. **Teclado virtual (IME)**: o Rust ainda não recebe a altura real do teclado virtual (`WindowInsets.Type.ime()`) nem os *safe-area insets* (notch, barra de gestos), fazendo com que campos de chat, login e busca fiquem cobertos pelo teclado.
3. **Movimentação sem teclado físico**: não há controle virtual (*virtual thumbstick* / botões de pulo, voo, subir/descer, agachar, interagir) na tela.
4. **HUD poluído em telas pequenas**: ilhas superiores, chat e dock disputam espaço horizontal sem modo retrátil.
5. **Interações dependentes de mouse**: clique direito (menu de contexto), *hover tooltips*, *drag-and-drop* de inventário/toolbar e colunas densas em tabelas.

---

## 2. Sistema de Design Mobile (Celular & Tablet)

### 2.1 Breakpoints e Formatos Suportados
Expandir `FormFactor` em `crates/aurora-viewer/src/touch.rs` para considerar **largura, altura e orientação**:

| Perfil | Dimensão lógica (dp) | Layout de HUD | Comportamento das Janelas (`Floater`) |
|---|---|---|---|
| **Phone Portrait** (`CompactPortrait`) | `w < 600`, `h > w` | HUD retrátil no topo + pad virtual inferior esquerdo + ações inferior direito | **Bottom Sheet** (50% ou 92% da altura, arrastável para fechar) ou **Tela Cheia** — apenas 1 painel ativo por vez |
| **Phone Landscape** (`CompactLandscape`) | `h < 500`, `w > h` | HUD estilo jogo (cantos livres para polegares, barras finas retráteis) | **Side Drawer** (gaveta lateral esquerda ou direita, 45%–60% da largura) preservando visão do mundo 3D |
| **Tablet / Foldable** (`Medium`) | `600 <= w < 840` | HUD completo com dock rolável + pad virtual opcional | **Side Sheet** ou painel lateral ancorado (380–440dp), até 2 painéis sem sobreposição |
| **Large Tablet** (`Expanded`) | `w >= 840` | HUD desktop/tablet híbrido + controles touch opcionais | **Split-View / Multi-pane** (ex.: lista + detalhe lado a lado) ou floaters redimensionáveis com alvos touch |

### 2.2 Regras de Ergonomia Touch
- **Alvos de toque**: mínimo `44–48dp` para botões primários, abas, fechar (`✕`) e controles de jogo; `36–40dp` mínimo absoluto em barras secundárias.
- **Zona dos polegares**: controles de uso contínuo (movimento, câmera, pulo/voo, microfone, chat rápido, câmera rápida) nos terços inferiores esquerdo e direito.
- **Telas retráteis (*Collapsible / Auto-Hide HUD*)**:
  - Botão discreto de **"Modo Cinema / Ocultar HUD"** e recolhimento automático de barras secundárias ao caminhar ou orbitar a câmera.
  - Abas horizontais com rolagem fluida por arrasto (*kinetic horizontal scroll*) em todas as janelas com múltiplas abas (`Options`, `Inventory`, `People`, `Profile`, `About`, `Build`).
- **Substituição de gestos de desktop**:
  - **Clique direito** -> **Toque longo (500ms)** com feedback visual radial + vibração háptica curta (`VibrationEffect`).
  - **Hover tooltip** -> Rótulo inline, ícone `(i)` tocável ou subtítulo compacto.
  - **Drag-and-drop** -> Modo de seleção / menu de ação ("Vestir", "Mover para…", "Dar para…", "Fixar na barra").
  - **Botão Voltar do Android** -> Pilha determinística: fecha teclado -> fecha menu de contexto -> fecha modal -> recolhe sheet/gaveta atual -> sai do mouselook -> pede confirmação de saída.

---

## 3. Ordem de Prioridades de Implementação

### Prioridade 0 — Fundação Responsiva, Safe Areas e IME (Pré-requisito de tudo)
**Objetivo:** Garantir que a infraestrutura de tela, teclado virtual e container de janelas funcione em qualquer celular ou tablet antes de adaptar telas individuais.

1. **Ponte JNI de `WindowInsets` (IME + Cutout + Navigation Bars)**
   - **Arquivos:** `packaging/android/app/src/main/java/org/auroraviewer/aurora/AuroraActivity.java`, `crates/aurora-viewer/src/android.rs`
   - Registrar `View.setOnApplyWindowInsetsListener` (e `WindowInsetsAnimation.Callback` na API 30+) para enviar continuamente ao Rust:
     - `ime_bottom_px` (altura atual do teclado virtual aberto);
     - `safe_insets` (`top`, `bottom`, `left`, `right` para notch/câmera/bordas curvas).
   - Adicionar métodos JNI `showSoftKeyboard()` e `hideSoftKeyboard()` acionados automaticamente quando um `egui::TextEdit` ganha/perde foco no Android.
2. **Motor de Layout Responsivo (`ResponsiveLayout`)**
   - **Arquivos:** `crates/aurora-viewer/src/touch.rs`, `crates/aurora-viewer/src/ui/layout.rs`
   - Evoluir `FormFactor` para distinguir `CompactPortrait`, `CompactLandscape`, `Medium`, `Expanded`.
   - Calcular `usable_rect` descontando `safe_insets` e `ime_bottom_dp`.
   - Atualizar `apply_touch_style()` com alvos de `44–48dp`, fontes legíveis por DPI, espaçamentos confortáveis e scrollbars discretas ativadas por toque/arrasto direto no conteúdo (*drag-to-scroll* em todas as `ScrollArea`).
3. **Reestilização Central de `Floater` e `Modal`**
   - **Arquivos:** `crates/aurora-viewer/src/ui/widgets.rs`, `crates/aurora-viewer/src/ui/widgets/workspace.rs`, `crates/aurora-viewer/src/ui/sheet.rs`
   - Em `CompactPortrait`: transformar `Floater` automaticamente em **Bottom Sheet / Full-Screen Sheet** com alça superior (*drag handle*) para puxar e fechar, cabeçalho touch (`48dp`), sem bordas de redimensionamento desktop, e gerenciador de janela única ativa (abrir uma nova recolhe a anterior na pilha).
   - Em `CompactLandscape`: transformar `Floater` em **Side Drawer** (gaveta lateral deslizável à direita ou esquerda, ocupando ~50% da largura) deixando o mundo 3D visível ao lado.
   - Em `Medium`/`Expanded`: manter floaters ou side-sheets ancorados com *clamping* rigoroso dentro de `usable_rect`.
   - Em `Modal`: centralizar acima do teclado (`usable_rect`), com largura fluida e botões empilhados verticalmente em telas estreitas.
4. **Abas e Barras com Rolagem Horizontal Nativa**
   - **Arquivos:** `crates/aurora-viewer/src/ui/widgets/pills.rs`, `crates/aurora-viewer/src/ui/widgets/workspace.rs`
   - Criar helper reutilizável `horizontal_scrollable_row` (com *fade* nas bordas indicando conteúdo extra) para substituir `horizontal_wrapped` excessivo em abas e barras de ferramentas no mobile.

---

### Prioridade 1 — Controles de Jogo Mobile (Virtual Pad) e HUD Retrátil
**Objetivo:** Permitir jogar, explorar e controlar o avatar com naturalidade apenas com os polegares, como em jogos 3D mobile (Genshin / Roblox / Minecraft / VRChat Mobile).

1. **Pad Virtual de Movimento e Câmera (`ui/touch_pad.rs`)**
   - **Arquivos:** novo `crates/aurora-viewer/src/ui/touch_pad.rs`, integração em `crates/aurora-viewer/src/app.rs` e `crates/aurora-viewer/src/touch.rs`
   - **Analógico Virtual Esquerdo (Thumbstick)**:
     - Modo *Floating* (aparece onde o polegar esquerdo toca na metade esquerda inferior) ou *Fixed* (configurável em Preferências).
     - Zona morta (*deadzone*), intensidade proporcional (andar devagar -> andar -> correr), e gesto de arrastar para cima com trava de **Auto-Run**.
     - Conectar diretamente aos controles de movimento do agente (`forward/back/strafe/turn`).
   - **Cluster de Ações no Polegar Direito**:
     - Botões circulares translúcidos estilo jogo (`48–56dp`): **Pular**, **Voar / Pousar** (quando voando, exibe botões **Subir / Descer**), **Agachar / Sentar / Levantar**, **Interagir (contextual)** quando há objeto/assento próximo.
     - Botão rápido de **Mouselook / 1ª Pessoa** e **Resetar Câmera**.
   - **Prevenção de Conflito com Câmera Touch (`TouchNav`)**:
     - Filtrar toques por `finger_id`: o dedo no thumbstick nunca move a câmera, enquanto o polegar direito livre gira a câmera simultaneamente (*dual-thumb control*).
2. **HUD Mobile Retrátil (Estilo Jogo)**
   - **Arquivos:** `crates/aurora-viewer/src/ui/bars.rs`, `crates/aurora-viewer/src/ui/bars/islands.rs`, `crates/aurora-viewer/src/ui/bars/dock.rs`, `crates/aurora-viewer/src/ui/bars/side.rs`
   - **Barra Superior Compacta e Retrátil**:
     - Esquerda: botão do logo OliViewer (abre gaveta principal de menu) + pílula compacta de Local/Região (tocar abre Detalhes do Local / Teleporte).
     - Direita: mini-indicadores (FPS/sinal, notificações com badge, botão de **Recolher/Expandir HUD**, microfone/voz).
   - **Dock Inferior / Gaveta de Ferramentas Rolável**:
     - Em celular: barra compacta retrátil (ou botão flutuante "Menu/Dock" expansível) com rolagem horizontal fluida e botões proporcionais ao tamanho da tela (`44–48dp`).
     - Reordenação e escolha de atalhos favoritos no dock via tela de edição touch (sem precisar de drag-and-drop difícil).
   - **Mini-Chat Overlay (estilo jogo)**:
     - Últimas 2–4 mensagens flutuantes semitransparentes no canto superior/inferior esquerdo que somem suavemente, com toque para expandir o chat completo.

---

### Prioridade 2 — Telas de Digitação Responsivas e Fluxos de Entrada (Teclado Virtual)
**Objetivo:** Eliminar completamente campos escondidos atrás do teclado virtual e tornar a digitação rápida e confortável em retrato e paisagem.

1. **Componente de Entrada Flutuante Acima do Teclado (`ImeComposer`)**
   - **Arquivos:** `crates/aurora-viewer/src/ui/widgets/text_field.rs`, `crates/aurora-viewer/src/ui/chat.rs`, `crates/aurora-viewer/src/ui/chat/`
   - Quando o teclado virtual abre (`ime_bottom_dp > 0`), ancorar a barra de digitação (chat local, IM, busca) **imediatamente acima do teclado** com botão grande de **Enviar** (`48dp`), seletor de **Emoji**, **Sussurrar / Falar / Gritar** e botão de fechar teclado.
   - Em `CompactLandscape` (onde o teclado ocupa >50% da altura), ativar automaticamente um **composer compacto em faixa única** no topo do teclado para que o usuário veja o que está digitando sem perder o contexto.
2. **Tela de Login e Gerenciador de Grids Mobile**
   - **Arquivos:** `crates/aurora-viewer/src/ui/login.rs`, `crates/aurora-viewer/src/ui/login/carousel.rs`, `crates/aurora-viewer/src/ui/login/credentials_form.rs`, `crates/aurora-viewer/src/ui/grids.rs`
   - Scroll automático para manter o campo focado visível acima do teclado virtual.
   - Carrossel de contas com gesto de *swipe* horizontal suave e cartões dimensionados para `CompactPortrait` e `CompactLandscape`.
   - Seletor de Grid (`OliGrid`, Second Life, Aditi, Gerenciar Grids) com botões de `48dp` e abertura limpa do gerenciador de grids em tela cheia no celular.
3. **Auto-scroll de Formulários ao Focar `TextEdit`**
   - **Arquivos:** `crates/aurora-viewer/src/ui/keyboard_focus.rs`, `crates/aurora-viewer/src/ui/widgets.rs`
   - Sempre que um campo de texto receber foco em qualquer janela (`Profile`, `Options`, `Search`, `Scripts`, `Landmark`), chamar `scroll_to_rect` ajustado ao `usable_rect` acima do IME.

---

### Prioridade 3 — Janelas Sociais e Interação com o Mundo (Uso Diário)
**Objetivo:** Adaptar as janelas mais usadas durante uma sessão normal no mundo virtual.

1. **Chat Local, IMs e Conversas (`ui/chat.rs`, `ui/chat/*`, `ui/emoji.rs`)**
   - Em celular: padrão **Lista de Conversas -> Tela da Conversa** (navegação em 2 níveis com botão Voltar), em vez de painel dividido estreito. Em tablet (`Medium`/`Expanded`): split-view lista + chat.
   - Seletor de emojis em grade touch (`44dp` por emoji) com abas horizontais roláveis.
2. **Menu de Contexto Touch / Radial (`ui/context.rs`, `ui/object_actions.rs`)**
   - Substituir menus cascata pequenos de desktop por **Action Sheet inferior** (ou menu radial touch ao segurar em um objeto/avatar no mundo 3D): "Tocar", "Sentar", "Inspecionar", "Perfil", "Pagar", "IM", "Mais…".
3. **Pessoas, Amigos, Radar e Grupos (`ui/people.rs`, `ui/contacts.rs`, `ui/profile.rs`, `ui/group_profile.rs`)**
   - Abas roláveis horizontalmente (`Perto`, `Amigos`, `Grupos`, `Bloqueados`).
   - Itens de lista com altura `56dp` (avatar + nome + distância/status + botão de ação rápida).
   - Perfil do residente e perfil de grupo em layout vertical de cartões com botões grandes de ação (`IM`, `Teleporte`, `Adicionar Amigo`, `Pagar`).
4. **Notificações, Convites e Diálogos do Servidor (`ui/notifications.rs`, `ui/notification_center.rs`, `ui/friend_prompt.rs`, `ui/call_invite.rs`, `ui/pay.rs`, `ui/region_message_prompt.rs`)**
   - Toasts no topo respeitando `safe_insets.top` com gesto de *swipe* para dispensar.
   - Central de notificações em gaveta deslizável.
   - Janela de pagamento (`Pay`) com botões grandes de valores rápidos em grade 2x2.

---

### Prioridade 4 — Inventário, Aparência, Mapa e Exploração
**Objetivo:** Tornar confortáveis as janelas ricas em conteúdo que hoje dependem de árvores densas ou telas largas.

1. **Inventário e Vestuário (`ui/inventory.rs`, `ui/inventory/*`, `ui/appearance.rs`, `ui/ao.rs`, `ui/gestures.rs`)**
   - Em celular: navegação por **breadcrumbs** (pasta atual no topo + botão voltar) ou árvore com recuo compacto e linhas de `48dp`, em vez de subpastas aninhadas estreitas.
   - Barra de busca fixa no topo com filtros em *chips* horizontais roláveis (`Todos`, `Vestindo`, `Recentes`, `Objetos`, `Roupas`, `Landmarks`).
   - Ações por toque no item abrem barra inferior de ações (`Vestir`, `Anexar`, `Remover`, `Renomear`, `Compartilhar`, `Lixeira`), substituindo clique direito e drag-and-drop.
2. **Mapa Mundial, Minimapa, Lugares e Busca (`ui/worldmap.rs`, `ui/minimap.rs`, `ui/places.rs`, `ui/destinations.rs`, `ui/search.rs`, `ui/area_search.rs`)**
   - **Minimapa**: widget circular ou quadrado arredondado no canto superior direito (estilo jogo), recolhível com um toque e expansível para o Mapa Mundial.
   - **Mapa Mundial**: gestos nativos de 1 dedo (arrastar mapa) e 2 dedos (pinça para zoom), barra de busca no topo e cartão inferior retrátil com botão grande **"Teleportar"**.
   - **Busca / Guia de Destinos**: cartões em grade responsiva (1 coluna em celular retrato, 2 em paisagem, 3+ em tablet).
3. **Câmera, Foto e Voz (`ui/snapshot.rs`, `ui/photo_reel.rs`, `ui/viewfinder.rs`, `ui/voice_controls.rs`, `ui/audio.rs`)**
   - Modo Câmera/Foto em tela cheia estilo app de câmera de celular (obturador grande, filtros/ajustes em faixa horizontal rolável na base).
   - Controles de Voz e Áudio com sliders largos (`48dp` de área de toque) e botão Push-to-Talk flutuante opcional no HUD.

---

### Prioridade 5 — Preferências, Configurações e Janelas Auxiliares
**Objetivo:** Adaptar telas de configuração e utilitários para leitura e ajuste fáceis no toque.

1. **Preferências (`ui/options.rs`, `ui/options/*`) e Quick Prefs (`ui/quick_prefs.rs`)**
   - Em celular: menu de categorias em lista principal -> página da categoria (ou abas horizontais roláveis no topo), em vez de barra lateral fixa que esmaga o conteúdo.
   - Sliders com valor legível e botões `−` / `+` opcionais para ajuste fino com o dedo.
   - Adicionar aba dedicada **"Controles Mobile / Touch"**: sensibilidade da câmera, tipo do pad virtual (fixo/flutuante), tamanho e opacidade dos botões, modo canhoto/destro, auto-ocultar HUD, escala de UI.
2. **Sobre (`ui/about.rs`), Estatísticas (`ui/perf.rs`), Ambiente/Clima (`ui/environment.rs`, `ui/weather_window.rs`), Land (`ui/land/*`)**
   - Todos se beneficiam automaticamente do `Floater` responsivo (P0) + abas horizontais roláveis + cartões empilhados em 1 coluna no `CompactPortrait`.

---

### Prioridade 6 — Ferramentas de Construção, Edição e Avançadas (Touch-Adapted)
**Objetivo:** Garantir que até recursos avançados sejam utilizáveis em celular/tablet sem quebrar a tela.

1. **Ferramenta de Construção (`build/ui/*`) e Textura/Cor (`ui/texture_picker.rs`, `ui/color_picker.rs`)**
   - Em `CompactLandscape` / Tablet: painel lateral retrátil fino à direita deixando o objeto 3D livre no centro para manipulação touch dos gizmos.
   - Inputs numéricos (`numeric_drag.rs`) com suporte a incremento por toque (`−` / `+`) e teclado numérico.
2. **Editor de Scripts, Notecards, Poser, Director (`ui/scripts.rs`, `ui/director.rs`, `ui/dev_toolbox.rs`)**
   - Editor de texto/notecard em tela cheia respeitando `ime_bottom_dp` com barra auxiliar acima do teclado (Desfazer, Salvar, Buscar).
   - Ferramentas complexas (Director, Dev Toolbox) com layout recolhível em gaveta e aviso suave em telas muito pequenas sugerindo modo paisagem ou tablet.

---

### Prioridade 7 — Polimento para Tablets, Dobráveis e Testes Automatizados
**Objetivo:** Refinar a experiência em todos os formatos de tela e garantir ausência de regressões no desktop.

1. **Multi-pane em Tablets e Dobráveis (`Medium` / `Expanded`)**
   - Chat (lista + conversa), Inventário (árvore + prévia/ações), Preferências (categorias + conteúdo) e Pessoas (lista + perfil) em 2 colunas automáticas.
   - Transição suave ao rotacionar o aparelho ou abrir/fechar tela dobrável sem perder estado de janelas abertas.
2. **Feedback Háptico e Visual**
   - Vibração curta no long-press, trava de Auto-Run, clique de botão de ação e notificações críticas.
3. **Cenários de Teste e Verificação**
   - Criar variável de teste desktop (ex.: `AURORA_MOBILE_PREVIEW=phone-portrait|phone-landscape|tablet`) para simular resoluções, `FormFactor`, `safe_insets`, teclado virtual (`IME`) e pad virtual diretamente no Windows/Linux durante o desenvolvimento, permitindo capturas automáticas (`AURORA_CAPTURE`) e testes rápidos sem precisar gerar APK a cada ajuste visual!

---

## 4. Resumo Visual do Cronograma Sugerido

| Fase | Foco | Impacto no Usuário | Dependência |
|---|---|---|---|
| **P0** | Insets/IME JNI + `FormFactor` + `Floater`/`Modal` responsivos + rolagem horizontal | Resolve 70% das quebras de layout em todas as janelas de uma só vez | Nenhuma |
| **P1** | Virtual Pad (thumbstick + ações) + HUD retrátil estilo jogo | Torna o app realmente jogável e navegável no celular | P0 |
| **P2** | Digitação acima do teclado virtual (Chat, Login, Grids, auto-scroll) | Torna login e comunicação confortáveis com teclado virtual | P0 |
| **P3** | Chat 2 níveis, Action Sheet touch (long-press), Pessoas, Perfis, Notificações | Experiência social completa no toque | P0, P2 |
| **P4** | Inventário touch, Minimapa/Mapa com pinça, Câmera/Foto, Voz | Exploração, personalização e mídia nativas para mobile | P0, P1 |
| **P5** | Preferências (com aba Touch/Mobile) e janelas auxiliares | Controle total de ergonomia e ajustes pelo usuário | P0 |
| **P6** | Build, Color/Texture Picker, Scripts/Notecards, ferramentas avançadas | Paridade funcional completa em tablets e celulares | P0, P2 |
| **P7** | Split-view tablets/dobráveis, hápticos e modo preview mobile no desktop | Acabamento profissional e manutenção sustentável | P0–P6 |
