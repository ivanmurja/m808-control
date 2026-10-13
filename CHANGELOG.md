# Changelog

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
