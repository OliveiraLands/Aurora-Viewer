# Análise do Crash no Android (`GPU device lost` após login na região)

## 1. Resumo do Problema

Ao executar o Aurora Viewer no Android (`org.auroraviewer.aurora`), o aplicativo inicializa normalmente, solicita a permissão `RECORD_AUDIO`, compila os 174 pipelines Vulkan e exibe a tela de login.  
Após realizar o login e entrar na região (`Oliveira Bay`), o app carrega o handshake, o ambiente (EEP), o cache de objetos e começa a baixar os assets da cena.

Entre **1,0 s e 1,7 s após a chegada na região (`arrival`)** — e aproximadamente **150 ms a 175 ms após o `first byte` do `asset fetch`** — a GPU sofre um fault fatal e retorna `VK_ERROR_DEVICE_LOST`:

```text
ERROR aurora_render::main_thread: GPU device lost (Unknown): Device is lost
ERROR aurora_viewer::app::gpu_stall: the graphics card is lost: recovering on a new window and device
WARN  aurora_render::renderer: GPU device lost: the window surface is left to the process
```

Em seguida, como `recover_from_gpu_loss` retorna `false` no Android (`crates/aurora-viewer/src/app/gpu_recovery.rs`), o viewer inicia o encerramento da sessão/janela (ocasionalmente exibindo a tela de login sobreposta ao minimapa/nome da região durante a desmontagem) e fecha o processo (às vezes acompanhado de `FORTIFY: pthread_mutex_lock called on a destroyed mutex` na thread `GPU completion` ou `ANR`).

---

## 2. Ambiente e Configuração Observados nos Logs

- **Plataforma:** Android (`aarch64-linux-android`, `target_sdk_version=35`, `minSdk 26`), resolução `1220x2712`, `60 Hz`
- **GPU / Driver:** `Mali-G615 MC6` via Vulkan (`v1.r44p1-01eac0.999bbcba36b4ca04774e2b05d5802dc0`)
- **Memória reportada pelo driver:** `100 MB dedicated + 11402 MB shared`
- **Descritores:** `500000 in an array, 49992 sampled per stage, 4294967295 in all pools, 512 texture pages`
- **Apresentação:** `Mailbox (vsync off, surface supports [Mailbox, Fifo])`
- **Recursos ativos/desativados no renderer:**
  - `BC7 compression`: `off (GPU support: false)`
  - `GPU culling / MULTI_DRAW_INDIRECT_COUNT`: `off` (draw lists construídas na CPU)
  - `MSAA / SSR / G-buffer`: `1 sample(s), SSR off, G-buffer off`
  - `Projector shadows`: `requested 0, effective 0, shelter map 0`
  - `Water depth clamp`: `hardware` (`DEPTH_CLIP_CONTROL` habilitado)
  - `TIMESTAMP_QUERY` e `blend_exact`: `off` no Android

---

## 3. O Que Já Foi Isolado com Certeza

### 3.1. Diagnóstico `android_frame_split` (última execução)

Em `crates/aurora-render/src/renderer.rs`, as submissões do frame foram divididas sequencialmente com `device.poll(wgpu::PollType::wait_indefinitely())` entre cada etapa (`upload`, `pre`, `scene`, `post`).

Resultado exato obtido no Logcat:

```text
ERROR aurora_render::main_thread: GPU device lost (Unknown): Device is lost
ERROR aurora_render::main_thread: GPU work at the loss: last submission frame 118, 0.154 s before, finished, frame 118 finished 0.000 s before, a fault found after that frame ended (not a hang)
ERROR aurora_render::renderer: android_frame_split LOST at frame 118 step 'scene' (poll=Ok(QueueEmpty), uploads=1, terrain=(0, 11), opaque=(220, 17), mask=(560, 0), blend=(860, 24))
```

**Conclusões comprovadas:**
1. **`upload` passou sem erro** (não é o envio de staging buffers nem cópias de upload).
2. **`pre` (`enc_pre`) passou sem erro** (não é shadow pass, depth prepass, oclusão Hi-Z, SSAO, reflexões, probes nem impostors).
3. **`post` (`encoder` de pós-processamento + `egui`) nem chegou a ser submetido** (não é TAA/FSR, DoF, exposure, bloom, tonemap nem egui).
4. **O fault ocorre especificamente dentro de `enc_scene`** (`crates/aurora-render/src/renderer.rs`, linhas `10228`–`11353`), no frame `118`, quando há cena real sendo desenhada (`terrain=(0, 11)`, `opaque=(220, 17)`, `blend=(860, 24)`).
5. **Não é timeout/hang de GPU:** os primeiros ~100+ frames pós-login e pré-assets rodam normalmente; o fault ocorre imediatamente quando novos assets/objetos entram em renderização.

### 3.2. Hipóteses já testadas e descartadas como causa única

As alterações abaixo já estão aplicadas no código e **não eliminaram** o crash:
- Desabilitar `TIMESTAMP_QUERY`, `blend_exact` (`EARLY_TEST`, `TWO_SOURCES`, `WIDE_RANGE`) e `GpuCull`.
- Reescrever loops de bitmask em `common.wgsl` e `object.wgsl` para contadores limitados.
- Desabilitar `dedicated::texture` (`crates/aurora-render/src/dedicated.rs`) no Android para usar `wgpu::Device::create_texture`.
- Desabilitar `PARTIALLY_BOUND_BINDING_ARRAY`, limitar `texture_page_limit` a `512` (`main_thread.rs`) e `max_layers` a `256` (`textures.rs`).
- Corrigir a permissão `RECORD_AUDIO` em tempo de execução antes de iniciar o app (`android.rs`).

---

## 4. Anatomia de `enc_scene` e Possibilidades para o Fault

Tudo o que executa dentro de `enc_scene` está entre `let enc_pre = std::mem::replace(...)` e `let enc_scene = std::mem::replace(...)` em `crates/aurora-render/src/renderer.rs`:

1. **`water exclusion mask`**: render pass limpando `self.targets.water_mask` e desenhando máscaras de exclusão de água.
2. **`scene a`**: render pass principal desenhando:
   - `sky` (`self.pipelines.sky`)
   - `terrain` (`terrain`, `terrain_pbr`, `terrain_mask` — 11 draws no frame 118)
   - `opaque` / `mask` (`opaque`, `opaque_2s`, `mask`, `mask_2s` — 17 draws no frame 118)
   - `impostors` (`self.impostors.sprite`)
3. **`cutout` / `rain_wet`**: passes condicionais de recorte ou umidade de chuva.
4. **`blend under water`**: objetos transparentes abaixo da água + pass de profundidade.
5. **`scene copy`**: `copy_texture_to_texture` de `color_tex` (`HDR_FORMAT`) para `scene_copy_tex`.
6. **`scene b`**: render pass desenhando:
   - `ssr` / `rain_wet`
   - `water` (`water` ou `water_unclipped`)
   - `glow` (`glow_max`, `glow_suppress`, `glow`)
   - `blend` (24 draws transparentes no frame 118, incluindo `blend_depth` para meshes rigged / avatares)
   - `rain` e `particles`
7. **`drawn_px` (picking)** e **`depth_pick.encode_hover`**: leitura/cópia de profundidade sob o cursor no final de `enc_scene`.

---

## 5. Lista de Possibilidades (Ordenada por Probabilidade)

### Possibilidade A — Acesso fora dos limites (OOB) ou dado inválido em `storage buffer` / `vertex pulling` quando novas malhas chegam
> **Atualização de 19/11/2026:** menos provável do que o texto abaixo sugere. O naga limita (`Restrict`) os acessos a buffers quando o driver não tem `robustBufferAccess2`, e o índice de página de textura já é limitado por `texture_page_index`. Terreno, opacos e máscara alfa rodaram estáveis com milhares de desenhos. Ver a seção 7.

- **Por que faz sentido:** O crash acontece ~150–175 ms após `asset fetch: first byte` e logo após `mesh load: 56..62 downloading`. Enquanto os objetos ainda não tinham malha/textura carregada, os frames rodavam sem erro.
- **Mecanismo na Mali-G615:** Em GPUs Mali, um `storage buffer` (instâncias, faces, materiais, bones/skinning ou vértices) lido fora dos limites ou com índice não inicializado dentro do shader (`object.wgsl` / `terrain.wgsl`) causa MMU page fault instantâneo no hardware (`VK_ERROR_DEVICE_LOST` — *"a fault in the work of that frame"*).
- **Onde olhar:** `scene a` (`opaque`, `terrain`) e `scene b` (`blend`, especialmente meshes rigged / `skin bindings 207 used of 207`).

### Possibilidade B — Fault específico em shader ou recurso de `scene b` (`water`, `glow` ou `blend`)
- **Por que faz sentido:** No frame 118 havia `blend=(860, 24)` e a região tem água. `scene b` lê `scene_copy_tex` e `depth_ss` e executa `water.wgsl` / `object.wgsl` (pass de blend / glow / rigged blend depth).
- **Detalhe importante:** O log mostra `water interface depth clamp: hardware` (`wgpu::Features::DEPTH_CLIP_CONTROL`). Se `VK_EXT_depth_clip_enable` ou o pipeline de água/rigged-blend tiver comportamento anômalo no driver Mali `r44p1`, o fault dispara apenas quando `water` ou `blend` desenha pela primeira vez sobre a cena.

### Possibilidade C — Bug de compilação/execução do driver Mali (`r44p1`) em instruções específicas de `object.wgsl` / `terrain.wgsl` / `common.wgsl`
- **Por que faz sentido:** O `VK_ERROR_DEVICE_LOST` com *"fault found after that frame ended"* (sem timeout de watchdog) é típico de:
  - `NaN` / divisão por zero propagada para operação sensível ou loop infinito em shader que só é alcançado quando um material real (normal map, PBR glTF, emissive, environment reflection probe) ativa determinado branch no fragment shader;
  - `textureSampleLevel` / `textureLoad` com índice de página ou camada inválido no array `tex_pages` (`binding_array<texture_2d_array<f32>>`) quando uma textura real termina de decodificar e entra em uso.

### Possibilidade D — Transição/barreira Vulkan ou `scene copy` (`color_tex` -> `scene_copy_tex`) ou `depth_pick.encode_hover`
- **Por que faz sentido:** No Android, `color_tex` (`Rgba16Float`) usa compressão de framebuffer interna da arquitetura TBDR (AFBC) na Mali. Se `copy_texture_to_texture` (ou `depth_pick.encode_hover` lendo `depth_ss`) tiver alguma incompatibilidade de uso/layout ou formato no `wgpu-hal` Vulkan sobre Mali `r44p1`, o erro só aparece quando há pass de cena ativo.

### Possibilidade E — Uso de `PresentMode::Mailbox` na `Surface` do Android (`NativeActivity`)
- **Observação:** Nos logs anteriores sem `android_frame_split`, apareceu `QUEUE_BUFFER_TIMEOUT: ... fenceName: GPU completion, waitFenceTime: 758` junto com `Mailbox`.
- **Avaliação:** No último teste com `android_frame_split`, o fault ocorreu **antes** do `post` e do `present()`, provando que o `QUEUE_BUFFER_TIMEOUT` era efeito colateral do travamento da GPU dentro de `scene`. Ainda assim, usar `PresentMode::Fifo` no Android é recomendado para evitar pressão desnecessária no `BLASTBufferQueue`.

---

## 6. Plano Recomendado para Próxima Análise

1. **Subdividir `enc_scene` em checkpoints granulares** (1 único build):
   Isolar exatamente qual sub-etapa de `enc_scene` falha:
   - `water_mask`
   - `scene_a_sky`
   - `scene_a_terrain`
   - `scene_a_opaque`
   - `blend_under`
   - `scene_copy`
   - `scene_b_water`
   - `scene_b_glow`
   - `scene_b_blend`
   - `depth_pick`
2. **Ativar Vulkan Validation Layers (`VK_LAYER_KHRONOS_validation`) no APK de debug**:
   Copiar `libVkLayer_khronos_validation.so` do NDK para `packaging/android/app/src/main/jniLibs/arm64-v8a/` e habilitar `wgpu::InstanceFlags::debugging()` temporariamente no Android. Isso revela no Logcat qualquer erro de barreira, layout de imagem, descritor não vinculado ou acesso inválido antes do fault do driver.
3. **Limpar o diagnóstico temporário `android_frame_split` em `crates/aurora-render/src/renderer.rs`** após concluir o isolamento.

---

## 7. Resultados da investigação de 19/11/2026

Aparelho: Motorola edge 60 pro (Mali-G615 MC6, driver `v1.r44p1`), build `ci` (`opt-level = 0`), região de teste com ~1500 malhas, ~4000 desenhos opacos e ~750 faces com blend.

### 7.1. O que foi descartado

| Hipótese | Como foi testada | Resultado |
|---|---|---|
| Índice não uniforme em `binding_array<texture_2d_array>` | `page_count = 1` no buffer de localizações (tudo na página 0) | **Continuou caindo** |
| Uploads de textura (cópia ou staging) | `debug.aurora.skip` bit 32: `PageStore::replace*` devolve `false` | **Continuou caindo** |
| Validação do Vulkan | `VK_LAYER_KHRONOS_validation` 1.4.363.1 no APK, `InstanceFlags::debugging()` | A layer carregou. **Nenhum `VUID` ou `Validation Error` apareceu** nas leituras feitas (o buffer do logcat girava, então não é prova de ausência) |
| Os passes e os alvos de render (HDR, cópias, resolve) | bit 64: nenhum desenho de malha, todos os passes mantidos | **Estável** (cena vazia, só céu, nome e controles) |
| Terreno, opacos (4070), máscara alfa (115) | bits 128, 256, 512 | **Estáveis** |
| Água, brilho, SSR, chão molhado, chuva e partículas | máscara 1920 (famílias de malhas omitidas) | **Estáveis** |

### 7.2. O que foi localizado: a família de blend

Com terreno, opacos e máscara alfa ligados, a GPU só caiu quando as faces com **blend** (`fs_blend`) foram ligadas (bit 1024). Uma cena com **uma única face de blend** (`blend=(2060, 1)`) já derrubou a GPU. Os crashes em `scene`, `scene_b` e `cutout_rain_blend_under` eram sempre o `fs_blend`: o passo acusado variava porque o blend é desenhado em vários passos, incluindo o dos blends sob a água.

### 7.3. Bisect dentro do `fs_blend`

`DIAG_BLEND` (constante injetada em `object_shader_source`, propriedade `debug.aurora.blend`, lida na abertura do app). Bits: 1 portal, 2 descarte antecipado do glare, 4 alfa do derez (`rez_alpha`), 8 recorte de água, 16 cor fixa no lugar do material e da iluminação.

| `debug.aurora.blend` | O que ficou ligado | Resultado |
|---|---|---|
| 16 | nada do material (cor fixa) | estável (>2,5 min) |
| 15 | material e iluminação; sem portal, glare, derez e recorte | estável (5,5 min) |
| 13 | só o glare | estável (6,5 min) |
| 7 | só o recorte de água | estável (5 min) |
| **11** | **só o derez** | **caiu em ~2 min** |
| 0 (com `rez_alpha_of`) | tudo | **caiu em ~3 min** |
| **4** | tudo, menos o derez | **estável** (5 min 15 s, depois ~4 min num dispositivo recriado; ~9 min no total) |

**Hipótese falsificada:** a cópia por valor do `DrawRecord` e do `VsOut` na chamada de `rez_alpha` estourando a pilha da Mali. Criar `rez_alpha_of(rez: vec4<u32>, world_z: f32)` (só o que a função lê) não evitou a queda.

### 7.4. Direção indicada

1. **Suspeito principal: o `rez_alpha` (alfa do derez) no `fs_blend`.** Todas as rodadas estáveis tinham o derez desligado, e as duas com ele ligado caíram. A função, com `0 derez` na cena, devolve 1.0 na primeira linha, então a causa não é uma conta: parece ser **geração de código do compilador do driver** (r44p1) para o shader com esse trecho. Isso é hipótese, não prova.
2. **Contorno para Android:** não aplicar o `rez_alpha` no `fs_blend` (custo visual: o fade do derez deixa de valer nos faces com blend). Confirmar com rodadas de 6 minutos ou mais, em mais de uma região, e com o avatar de outro dispositivo.
3. **Causa raiz:** compilar o `fs_blend` com o Mali Offline Compiler (Arm), comparar a versão com e sem `rez_alpha` (registradores, pilha, instruções), e relatar à Arm com o SPIR-V mínimo.
4. **Provas que ainda faltam:** o portal (`portal_surface`, bit 1) nunca foi testado sozinho. As amostras são poucas e o crash não tem hora fixa (de 1 a 3 min nos casos que caíram), então qualquer conclusão precisa de repetição.

### 7.5. Limitações das medidas

- O `android_frame_split` atribui a perda ao passo em que o poll a percebe. Uma falta assíncrona pode vir de um comando anterior.
- Cada "estável" é uma rodada de alguns minutos, não uma garantia.
- Minimizar o app **recria o renderizador** (cerca de 14 s) e a cena é recarregada; conta como reinício do teste na GPU.

### 7.6. Achados laterais (fora do crash)

- **Primeira abertura trava a tela** (ANR do Android) enquanto ~174 pipelines são compilados sem aviso (`pipeline cache: none yet for this GPU`). Falta um aviso de progresso e compilar sem bloquear a thread principal; interromper nesse momento pode deixar arquivos iniciais incompletos. Tarefa separada.
- **Girar a tela (retrato para paisagem) derrubou o app** uma vez. Não foi investigado; pode ser outra falha (recriação da surface e dos alvos).
- `mali_gralloc: Unrecognized and/or unsupported format 0x38` aparece no começo da execução. Não parece relacionado, mas não foi investigado.
- `adb logcat -G` **apaga** o conteúdo do buffer. Salvar o log em arquivo logo depois da queda (`adb logcat -d > arquivo.log`).

## 8. Como reproduzir e usar os diagnósticos

Build e instalação (da raiz do repositório):

```powershell
cargo ndk -t arm64-v8a --platform 26 -o packaging/android/app/src/main/jniLibs build --lib -p aurora-viewer --profile ci --locked
cd packaging/android; ./gradlew.bat installDebug
```

Propriedades (`adb shell setprop <nome> <valor>`, valem até reiniciar o aparelho):

- `debug.aurora.skip`: lida **a cada frame**, pode mudar com o app aberto. Bits: 1 SSR e chão molhado, 2 água, 4 brilho, 8 faces com blend (todos os caminhos), 16 sobreposições, chuva e partículas, 32 uploads de textura, 64 todos os desenhos de malha, 128 terreno, 256 opacos, 512 máscara alfa, 1024 blend, 2048 blend rigged (avatar), 4096 blend não rigged.
- `debug.aurora.blend`: lida **só na abertura**, exige fechar e reabrir o app (`adb shell am force-stop org.auroraviewer.aurora`).

Cada mudança no log aparece como `android diagnostic: debug.aurora.skip = N` / `debug.aurora.blend = N`. Se a linha não aparecer, a máscara não foi aplicada.

## 9. Estado do código (diagnósticos temporários a remover)

Alterações **não commitadas** nesta branch (`feat/aurora-android`), além das do port do Android que já existiam:

- `crates/aurora-render/src/renderer.rs`: `scene_cut!` / `scene_parts` / `part_labels` (divisão de `enc_scene` em partes), `android_skip`, `android_blend_diag`, os bits de `skip` em `draw`, `draw_from`, `draw_sided`, `blended`, `rest`, SSR, água e brilho, e o fechamento `diag_skips`.
- `crates/aurora-render/src/textures.rs`: `ANDROID_FORCE_PAGE0` (desligada) e `shader_page_count`; o desvio do bit 32 em `replace` e `replace_staged`.
- `crates/aurora-render/src/lib.rs`: a injeção de `DIAG_BLEND` em `object_shader_source`.
- `crates/aurora-render/src/shaders/object.wgsl`: `DIAG_BLEND` e seus usos no `fs_blend`; as funções `rez_progress_of`, `rez_fade_front_of` e `rez_alpha_of`.
- `crates/aurora-render/Cargo.toml`: dependência `libc` para `target_os = "android"`.
- `packaging/android/app/src/main/jniLibs/arm64-v8a/libVkLayer_khronos_validation.so`: layer de validação (pasta no `.gitignore`).

**Ainda não verificado:** `cargo test --workspace` não foi rodado depois dessas mudanças. Os shaders de teste de `blend_tests.rs` e `blend_exact_tests.rs` montam trechos do `fs_blend` com stubs próprios e podem não conhecer `DIAG_BLEND` nem `rez_alpha_of`.

## 10. Para retomar em outro computador

1. Pegar a branch com o commit do trabalho em andamento (a mensagem sugerida está em `.scratch/commit-message.txt`, não versionado).
2. Conferir a conexão do aparelho (`adb devices`) e o `local.properties` do `packaging/android` (não versionado).
3. Instalar o APK e repetir o teste **`debug.aurora.blend = 4`** (tudo ligado, sem derez) por 6 minutos ou mais, em mais de uma região, para confirmar o contorno.
4. Testar o portal sozinho (`debug.aurora.blend = 14`, só o portal ligado) para fechar o bisect.
5. Com o contorno confirmado: aplicá-lo só para Android (por exemplo, condicionando o `rez_alpha` no `fs_blend`), remover os diagnósticos da seção 9, rodar `./scripts/check.ps1` e conferir o desktop em modo demo.
6. Abrir as tarefas laterais da seção 7.6 e, se a causa for do driver, relatar à Arm.
