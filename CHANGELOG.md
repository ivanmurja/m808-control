# Changelog

## 1.1.0 — 2026-10-13

### Novidades
- **Perfis** salvos no PC, aplicados pela janela ou pela bandeja
- **Troca automática de perfil** pelo programa em uso, por **horário** (dias e intervalo, inclusive
  cruzando a meia-noite) ou por **bateria fraca**, com perfil padrão
- **Exportar e importar perfis** (`.m808profiles`)
- **Bateria** do mouse na janela, no menu e no ícone da bandeja; aviso de bateria fraca
- **Ícone da bandeja dinâmico**: DPI atual na cor do nível ("1.6", "2K") ou bateria
- **Aviso na tela** ao trocar DPI, polling ou perfil (janela criada só durante o aviso)
- **Faixa RGB com velocidade e brilho contínuos** de 1 a 100%, além dos limites do app oficial
- **Atualização automática** pelas releases do GitHub, com assinatura verificada
- **Interface em inglês** (automático pelo idioma do sistema ou escolha manual)
- **Layout responsivo**: janela pode ficar bem menor; abas vão para o topo em telas estreitas
- Vista do mouse redesenhada, mais fiel ao Storm Pro (faixa RGB dos dois lados)

## 1.0.0 — 2026-10-13

Primeira versão estável.

### Recursos
- Níveis de DPI (100–16000, passo 100), quantidade de níveis no ciclo, nível ativo e cor de cada nível
- Vista de cima do Storm Pro com os 8 botões clicáveis para remapear: cliques, voltar/avançar,
  DPI +/−/ciclo, troca de polling rate, liga/desliga luzes, reset de fábrica, desativar
- Faixa RGB lateral: onda arco-íris, **onda colorida** (efeito do firmware que o app oficial não
  oferece), respiração, fixo, desligado — com velocidade e brilho em 10 níveis
- Luz da roda (indicador de DPI): fixa, respiração ou desligada
- **Faixa sincronizada com a cor do nível de DPI** ao trocar o DPI no mouse
- Polling rate 125/250/500/1000 Hz
- Backup e restauração da memória do mouse; backup automático antes de gravar a função de reset
- Ícone na bandeja com troca rápida de DPI; inicia com o sistema minimizado; instância única
- Acompanha em tempo real as trocas de DPI e polling feitas pelos botões do mouse
- Grava só os bytes alterados e confirma relendo a memória quando o mouse está acordando

### Limitações conhecidas
- Macros, combinações de teclas e tecla de teclado num botão ainda não são suportadas
- Apenas Windows; versão Linux planejada
- Instalador não assinado (aviso do SmartScreen)
