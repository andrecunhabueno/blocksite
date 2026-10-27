# Blocksite

Aplicativo gráfico em Rust para configurar controles locais no Windows. Ao iniciar sem argumentos, abre formulários para senha, bloqueio de sites, wallpaper e permissões. As senhas são solicitadas sem eco em fluxos de terminal e armazenadas como hashes Argon2id.

## Requisitos

- Windows 10/11
- Rust e Cargo
- Linker C/C++ para Windows: MSVC (Visual Studio Build Tools) ou MinGW-w64 configurado para o target Rust
- Terminal executado como administrador para `apply`, bloqueio de instalação e alterações em políticas do Windows

## Iniciar a interface gráfica

```powershell
cargo run
```

Na primeira abertura, o Windows solicitará permissão de administrador (UAC). Aprove a solicitação para abrir o formulário. Depois, use as seções **Sites**, **Wallpaper**, **Instalação e sistema** e **Sobre**. A aba Sobre exibe o logo, a versão Beta 0.1 e o crédito de desenvolvimento. A senha é digitada em uma janela protegida e não aparece na linha de comando. O Blocksite mantém a configuração no perfil do usuário que o iniciou, mesmo após a elevação.

A interface usa Windows Forms e PowerShell, incluídos no Windows. O código de controle e as regras continuam implementados em Rust.
O executável é compilado como aplicativo gráfico do Windows, portanto não abre uma janela de Prompt/console junto com o formulário.

O ícone também aparece na área de notificação do Windows. Minimizar ou fechar a janela a mantém nessa área; clique com o botão esquerdo no ícone ou escolha **Abrir Blocksite** para informar a senha e restaurar a janela. Se a senha for cancelada ou estiver incorreta, a janela continua oculta. Para sair normalmente, clique com o botão direito no ícone, escolha **Sair** e confirme a senha. O Windows pode colocar ícones na área oculta da bandeja; nesse caso, ajuste a visibilidade do Blocksite nas configurações da barra de tarefas. O Gerenciador de Tarefas pode encerrar processos diretamente e não oferece ao aplicativo um mecanismo confiável para exigir senha nessa ação. Para proteger o app contra usuários, use uma conta padrão do Windows e mantenha privilégios administrativos separados.

Na janela:

1. Crie uma senha no primeiro uso.
2. Em **Sites**, informe os domínios a bloquear e use **Salvar lista**. O botão **Liberar sites** pausa o bloqueio até você reativá-lo.
3. Em **Wallpaper**, escolha uma imagem e aplique o bloqueio.
4. Em **Instalação e sistema**, libere MSI e Configurações por 30 minutos ou bloqueie cada item novamente.

Na aba **Sites**, o Blocksite sempre bloqueia os domínios da lista. O botão **Salvar lista** grava e aplica as regras; para isso, abra o programa como administrador. **Liberar sites** pede a senha e remove temporariamente as regras; elas permanecem liberadas até clicar em **Bloquear sites**, que também pede a senha. O bloqueio padrão de sites adultos começa ativado e pode ser ligado ou desligado pela opção na mesma aba.

## Ícone da janela

O arquivo PNG original está em `assets\blocksite.png`. O arquivo `.ico` derivado, usado na janela e incorporado ao executável quando `windres` está disponível, fica nesta pasta:

```text
assets\blocksite.ico
```

O caminho completo no workspace atual é `D:\Perfil\Desktop\Blocksite\assets\blocksite.ico`. Após substituir o PNG, gere novamente o ICO e recompile para atualizar o ícone.

## Primeira execução pelo terminal

```powershell
cargo run -- init
```

Escolha uma senha com pelo menos oito caracteres. Para aplicar as políticas, abra um terminal como administrador:

```powershell
cargo run -- apply
```

## Bloqueio de sites

O modo `deny` adiciona os domínios indicados ao arquivo `hosts`:

```powershell
cargo run -- mode deny "facebook.com,tiktok.com"
cargo run -- apply
```

As alterações do Blocksite ficam entre marcadores próprios no arquivo `hosts`, sem substituir o conteúdo que já existia. O bloqueio cobre o domínio informado e `www`; outros subdomínios devem ser informados separadamente. Ao aplicar uma lista de bloqueio, o Blocksite inicia um servidor local em `127.0.0.1:80`; acessos HTTP aos domínios bloqueados mostram `src\block_page.html`.

A lista adulta embutida começa ativada e contém 20 domínios populares. Ela é uma lista inicial, não exaustiva, não recebe atualizações automáticas e pode ficar desatualizada. O mecanismo `hosts` não permite curingas: subdomínios adicionais, DNS sobre HTTPS, VPNs ou aplicativos com resolução própria podem contornar o bloqueio. Portanto, essa opção não substitui um filtro DNS/rede dedicado nem controles parentais do sistema.

**Limite de HTTPS:** um site HTTPS não pode ser redirecionado para uma página HTML local apenas com o arquivo `hosts`. Como o navegador exige um certificado válido para o domínio, nesses endereços a conexão segura falha em vez de mostrar a página Blocksite. Não instale certificados de interceptação para contornar isso.

O arquivo `hosts` do Windows não consegue bloquear toda a Internet e liberar somente uma lista de domínios, nem bloquear todos os subdomínios com curingas. Por isso, a interface gráfica oferece apenas o bloqueio dos domínios informados. Configurações antigas no modo `allow` devem ser salvas novamente pela interface para migrar para o bloqueio.

## Wallpaper e configurações

```powershell
cargo run -- wallpaper "C:\Users\Usuario\Pictures\fundo.jpg"
cargo run -- unlock-wallpaper
cargo run -- unlock-settings
cargo run -- lock-settings
```

Esses comandos pedem a senha configurada. O bloqueio de Configurações e wallpaper usa políticas do perfil atual do Windows.

## Instalação de programas

```powershell
cargo run -- unlock-install
cargo run -- lock-install
```

A liberação de MSI e Configurações dura 30 minutos. `unlock-settings` também libera somente as Configurações por 30 minutos. Um monitor em segundo plano reativa cada política ao expirar, mesmo que o terminal seja fechado.

**Limite importante:** a política `DisableMSI` do Windows controla instaladores MSI. Ela não impede todos os instaladores `.exe`, aplicativos portáteis, Microsoft Store ou usuários administradores. Para controle robusto, configure contas padrão e use AppLocker ou Windows Defender Application Control conforme a edição do Windows.

## Limitações de segurança

Este aplicativo configura políticas locais; ele não substitui controles de administração de dispositivos. Um administrador do computador pode alterar ou remover essas políticas e os arquivos locais. Para uso por crianças ou em ambiente corporativo, use uma conta padrão para o usuário e mantenha a senha administrativa fora do alcance dessa conta.

## Verificação

```powershell
cargo test
cargo fmt --check
```
