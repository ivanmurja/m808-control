# Assinatura do instalador (Windows)

Hoje o instalador não tem assinatura digital de código (Authenticode), por isso o Windows SmartScreen
mostra "O Windows protegeu o computador". Isso é diferente da assinatura do **updater**, que já existe:
ela garante que as atualizações automáticas vieram deste repositório.

## Opção recomendada: SignPath Foundation (grátis para open source)

A [SignPath Foundation](https://signpath.org) fornece certificado de assinatura gratuito para projetos
open source. O certificado fica com eles; o GitHub Actions envia o instalador para ser assinado.

### O que o mantenedor precisa fazer (uma vez)

1. Ler os requisitos em https://signpath.org/terms (projeto open source com licença OSI — MIT serve —,
   releases públicas, builds feitos por CI).
2. Fazer a inscrição em https://signpath.org/apply com o link do repositório
   `https://github.com/ivanmurja/m808-control`.
3. Após aprovado, no painel do SignPath:
   - criar o **Project** (`m808-control`) e uma **Signing Policy** (`release-signing`);
   - ligar o repositório GitHub como *Trusted Build System*;
   - gerar um **API token** de CI.
4. No GitHub, em *Settings → Secrets and variables → Actions*, criar:
   - `SIGNPATH_API_TOKEN` — o token do passo 3;
   - variável `SIGNPATH_ORGANIZATION_ID` — o id da organização no SignPath.

### O que muda no workflow

Com os secrets acima, o job de build passa a:

1. compilar o instalador sem publicar;
2. enviar o `.exe`/`.msi` com a action
   [`signpath/github-action-submit-signing-request`](https://github.com/SignPath/github-action-submit-signing-request);
3. publicar na release os arquivos já assinados (e assinar o updater sobre eles).

Esse trecho do workflow será adicionado quando a conta SignPath estiver aprovada, para poder ser
testado de ponta a ponta — um passo de assinatura sem credenciais válidas quebraria as releases.

## Alternativa: certificado próprio

Certificados OV/EV de autoridades comerciais custam a partir de ~US$ 200/ano e hoje exigem
token de hardware ou HSM na nuvem (ex.: Azure Trusted Signing). O Tauri aceita um comando de
assinatura em `bundle.windows.signCommand` no `tauri.conf.json`.
