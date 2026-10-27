param(
    [Parameter(Mandatory = $true)]
    [string]$Executable,
    [Parameter(Mandatory = $true)]
    [string]$ConfigDirectory,
    [Parameter(Mandatory = $true)]
    [string]$IconPath
)

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
[System.Windows.Forms.Application]::EnableVisualStyles()

if (-not ("BlocksiteShellIdentity" -as [type])) {
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

public static class BlocksiteShellIdentity
{
    [DllImport("shell32.dll", CharSet = CharSet.Unicode, PreserveSig = true)]
    public static extern int SetCurrentProcessExplicitAppUserModelID(string appId);
}
"@
}
$appIdResult = [BlocksiteShellIdentity]::SetCurrentProcessExplicitAppUserModelID("Blocksite.Desktop.App")
if ($appIdResult -ne 0) {
    [void][System.Windows.Forms.MessageBox]::Show(
        "Não foi possível registrar a identidade do Blocksite na barra de tarefas (0x{0:X8})." -f $appIdResult,
        "Identidade do Blocksite",
        [System.Windows.Forms.MessageBoxButtons]::OK,
        [System.Windows.Forms.MessageBoxIcon]::Warning
    )
}

$script:Executable = $Executable
$script:AppDirectory = $ConfigDirectory
$script:ConfigPath = Join-Path $script:AppDirectory "config.json"

function Invoke-Blocksite([string[]]$Arguments, [string[]]$PasswordLines = @()) {
    $start = New-Object System.Diagnostics.ProcessStartInfo
    $start.FileName = $script:Executable
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardInput = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.StandardOutputEncoding = [System.Text.Encoding]::UTF8
    $start.StandardErrorEncoding = [System.Text.Encoding]::UTF8
    $start.Arguments = (($Arguments | ForEach-Object {
        '"' + ($_ -replace '"', '\"') + '"'
    }) -join ' ')

    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $start
    if (-not $process.Start()) {
        return @{ Success = $false; Message = "Não foi possível iniciar o Blocksite." }
    }

    foreach ($line in $PasswordLines) {
        $process.StandardInput.WriteLine($line)
    }
    $process.StandardInput.Close()
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    $process.WaitForExit()
    $output = $stdoutTask.Result.Trim()
    $stderrText = $stderrTask.Result.Trim()
    $ok = $process.ExitCode -eq 0
    $process.Dispose()

    $message = if ($ok) { $output } else { $stderrText }
    if ([string]::IsNullOrWhiteSpace($message)) {
        $message = if ($ok) { "Concluído." } else { "O comando não foi concluído." }
    }
    return @{ Success = $ok; Message = $message }
}

function Start-BlocksiteAsync([string[]]$Arguments, [string[]]$PasswordLines = @()) {
    $start = New-Object System.Diagnostics.ProcessStartInfo
    $start.FileName = $script:Executable
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardInput = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.StandardOutputEncoding = [System.Text.Encoding]::UTF8
    $start.StandardErrorEncoding = [System.Text.Encoding]::UTF8
    $start.Arguments = (($Arguments | ForEach-Object {
        '"' + ($_ -replace '"', '\"') + '"'
    }) -join ' ')

    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $start
    if (-not $process.Start()) {
        $process.Dispose()
        throw "Não foi possível iniciar o Blocksite."
    }

    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    foreach ($line in $PasswordLines) {
        $process.StandardInput.WriteLine($line)
    }
    $process.StandardInput.Close()
    return @{
        Process = $process
        StdoutTask = $stdoutTask
        StderrTask = $stderrTask
    }
}

function Complete-BlocksiteAsync($Operation) {
    $process = $Operation.Process
    if (-not $process.HasExited -or
        -not $Operation.StdoutTask.IsCompleted -or
        -not $Operation.StderrTask.IsCompleted) {
        return $null
    }

    $output = $Operation.StdoutTask.Result.Trim()
    $stderrText = $Operation.StderrTask.Result.Trim()
    $ok = $process.ExitCode -eq 0
    $process.Dispose()

    $message = if ($ok) { $output } else { $stderrText }
    if ([string]::IsNullOrWhiteSpace($message)) {
        $message = if ($ok) { "Concluído." } else { "O comando não foi concluído." }
    }
    return @{ Success = $ok; Message = $message }
}

function Stop-BlocksiteAsync($Operation) {
    $process = $Operation.Process
    try {
        if (-not $process.HasExited) {
            $process.Kill()
            [void]$process.WaitForExit(3000)
        }
    } finally {
        $process.Dispose()
    }
}

function Get-PasswordInputs([string[]]$Prompts) {
    $dialog = New-Object System.Windows.Forms.Form
    $dialog.Text = "Confirmação de senha"
    $dialog.StartPosition = "CenterParent"
    $dialog.FormBorderStyle = "FixedDialog"
    $dialog.MaximizeBox = $false
    $dialog.MinimizeBox = $false
    $dialog.ClientSize = New-Object System.Drawing.Size(370, (100 + 58 * $Prompts.Count))

    $inputs = @()
    for ($i = 0; $i -lt $Prompts.Count; $i++) {
        $label = New-Object System.Windows.Forms.Label
        $label.Text = $Prompts[$i]
        $label.Location = New-Object System.Drawing.Point(18, (18 + 58 * $i))
        $label.AutoSize = $true
        [void]$dialog.Controls.Add($label)

        $input = New-Object System.Windows.Forms.TextBox
        $input.Location = New-Object System.Drawing.Point(18, (38 + 58 * $i))
        $input.Size = New-Object System.Drawing.Size(332, 24)
        $input.UseSystemPasswordChar = $true
        [void]$dialog.Controls.Add($input)
        $inputs += $input
    }

    $okButton = New-Object System.Windows.Forms.Button
    $okButton.Text = "Continuar"
    $okButton.Location = New-Object System.Drawing.Point(180, (48 + 58 * $Prompts.Count))
    $okButton.DialogResult = [System.Windows.Forms.DialogResult]::OK
    [void]$dialog.Controls.Add($okButton)

    $cancelButton = New-Object System.Windows.Forms.Button
    $cancelButton.Text = "Cancelar"
    $cancelButton.Location = New-Object System.Drawing.Point(270, (48 + 58 * $Prompts.Count))
    $cancelButton.DialogResult = [System.Windows.Forms.DialogResult]::Cancel
    [void]$dialog.Controls.Add($cancelButton)
    $dialog.AcceptButton = $okButton
    $dialog.CancelButton = $cancelButton

    if ($dialog.ShowDialog() -ne [System.Windows.Forms.DialogResult]::OK) {
        $dialog.Dispose()
        return $null
    }

    $values = @($inputs | ForEach-Object { $_.Text })
    $dialog.Dispose()
    return ,$values
}

function Show-Result([hashtable]$Result, [string]$SuccessTitle = "Blocksite") {
    $icon = if ($Result.Success) {
        [System.Windows.Forms.MessageBoxIcon]::Information
    } else {
        [System.Windows.Forms.MessageBoxIcon]::Warning
    }
    [void][System.Windows.Forms.MessageBox]::Show(
        $Result.Message,
        $SuccessTitle,
        [System.Windows.Forms.MessageBoxButtons]::OK,
        $icon
    )
}

function Get-PasswordOrCancel {
    $values = Get-PasswordInputs @("Digite sua senha:")
    if ($null -eq $values) { return $null }
    return $values[0]
}

function Invoke-ProtectedCommand([string[]]$Arguments, [string]$SuccessTitle = "Blocksite") {
    $password = Get-PasswordOrCancel
    if ($null -eq $password) { return $false }
    Show-Result (Invoke-Blocksite $Arguments @($password)) $SuccessTitle
    return $true
}

$status = Invoke-Blocksite @("status")
$configured = $status.Success -and $status.Message -match "Senha configurada:\s*True"
if (-not $configured) {
    $initial = Get-PasswordInputs @("Crie uma senha (mínimo 8 caracteres):", "Confirme a senha:")
    if ($null -eq $initial) { exit 0 }
    $created = Invoke-Blocksite @("init") $initial
    if (-not $created.Success) {
        Show-Result $created "Não foi possível configurar"
        exit 1
    }
}

$form = New-Object System.Windows.Forms.Form
$form.Text = "Blocksite"
$form.StartPosition = "CenterScreen"
$form.MinimumSize = New-Object System.Drawing.Size(820, 650)
$form.Size = New-Object System.Drawing.Size(820, 650)
$form.Font = New-Object System.Drawing.Font("Segoe UI", 10)
$form.BackColor = [System.Drawing.Color]::FromArgb(245, 247, 250)
$appIcon = $null
if (Test-Path $IconPath) {
    try {
        $appIcon = New-Object System.Drawing.Icon($IconPath)
        $form.Icon = $appIcon
    }
    catch {
        [void][System.Windows.Forms.MessageBox]::Show(
            "Não foi possível carregar o ícone em '$IconPath'. Use um arquivo .ico válido.",
            "Ícone do Blocksite",
            [System.Windows.Forms.MessageBoxButtons]::OK,
            [System.Windows.Forms.MessageBoxIcon]::Warning
        )
    }
} else {
    [void][System.Windows.Forms.MessageBox]::Show(
        "O arquivo do ícone não foi encontrado em '$IconPath'.",
        "Ícone do Blocksite",
        [System.Windows.Forms.MessageBoxButtons]::OK,
        [System.Windows.Forms.MessageBoxIcon]::Warning
    )
}
if ($null -eq $appIcon) { exit 1 }

$title = New-Object System.Windows.Forms.Label
$title.Text = "Blocksite"
$title.Font = New-Object System.Drawing.Font("Segoe UI Semibold", 20)
$title.Location = New-Object System.Drawing.Point(24, 18)
$title.AutoSize = $true
[void]$form.Controls.Add($title)

$subtitle = New-Object System.Windows.Forms.Label
$subtitle.Text = "Controles simples para este computador"
$subtitle.ForeColor = [System.Drawing.Color]::DimGray
$subtitle.Location = New-Object System.Drawing.Point(27, 58)
$subtitle.AutoSize = $true
[void]$form.Controls.Add($subtitle)

$tabs = New-Object System.Windows.Forms.TabControl
$tabs.Location = New-Object System.Drawing.Point(22, 94)
$tabs.Size = New-Object System.Drawing.Size(760, 455)
[void]$form.Controls.Add($tabs)

$sitesTab = New-Object System.Windows.Forms.TabPage
$sitesTab.Text = "Sites"
$wallpaperTab = New-Object System.Windows.Forms.TabPage
$wallpaperTab.Text = "Wallpaper"
$settingsTab = New-Object System.Windows.Forms.TabPage
$settingsTab.Text = "Instalação e sistema"
$aboutTab = New-Object System.Windows.Forms.TabPage
$aboutTab.Text = "Sobre"
[void]$tabs.TabPages.Add($sitesTab)
[void]$tabs.TabPages.Add($wallpaperTab)
[void]$tabs.TabPages.Add($settingsTab)
[void]$tabs.TabPages.Add($aboutTab)

$aboutLayout = New-Object System.Windows.Forms.TableLayoutPanel
$aboutLayout.Dock = [System.Windows.Forms.DockStyle]::Fill
$aboutLayout.ColumnCount = 1
$aboutLayout.RowCount = 3
$aboutLayout.ColumnStyles.Add((New-Object System.Windows.Forms.ColumnStyle(
    [System.Windows.Forms.SizeType]::Percent, 100
)))
$aboutLayout.RowStyles.Add((New-Object System.Windows.Forms.RowStyle(
    [System.Windows.Forms.SizeType]::Percent, 58
)))
$aboutLayout.RowStyles.Add((New-Object System.Windows.Forms.RowStyle(
    [System.Windows.Forms.SizeType]::Percent, 20
)))
$aboutLayout.RowStyles.Add((New-Object System.Windows.Forms.RowStyle(
    [System.Windows.Forms.SizeType]::Percent, 22
)))

$aboutLogo = New-Object System.Windows.Forms.PictureBox
$aboutLogo.Image = $appIcon.ToBitmap()
$aboutLogo.SizeMode = [System.Windows.Forms.PictureBoxSizeMode]::Zoom
$aboutLogo.Dock = [System.Windows.Forms.DockStyle]::Fill
$aboutLogo.Margin = New-Object System.Windows.Forms.Padding(0, 20, 0, 0)
[void]$aboutLayout.Controls.Add($aboutLogo, 0, 0)

$aboutVersion = New-Object System.Windows.Forms.Label
$aboutVersion.Text = "Versão Beta 0.1"
$aboutVersion.TextAlign = [System.Drawing.ContentAlignment]::MiddleCenter
$aboutVersion.Dock = [System.Windows.Forms.DockStyle]::Fill
$aboutVersion.Font = New-Object System.Drawing.Font("Segoe UI Semibold", 14)
[void]$aboutLayout.Controls.Add($aboutVersion, 0, 1)

$aboutAuthor = New-Object System.Windows.Forms.Label
$aboutAuthor.Text = "Desenvolvido por André Bueno"
$aboutAuthor.TextAlign = [System.Drawing.ContentAlignment]::MiddleCenter
$aboutAuthor.Dock = [System.Windows.Forms.DockStyle]::Fill
$aboutAuthor.Font = New-Object System.Drawing.Font("Segoe UI", 11)
[void]$aboutLayout.Controls.Add($aboutAuthor, 0, 2)
[void]$aboutTab.Controls.Add($aboutLayout)

$config = $null
if (Test-Path $script:ConfigPath) {
    try { $config = Get-Content $script:ConfigPath -Raw | ConvertFrom-Json } catch { $config = $null }
}

$siteHeader = New-Object System.Windows.Forms.Label
$siteHeader.Text = "Sites que o Blocksite deve bloquear"
$siteHeader.Font = New-Object System.Drawing.Font("Segoe UI Semibold", 12)
$siteHeader.Location = New-Object System.Drawing.Point(22, 20)
$siteHeader.AutoSize = $true
[void]$sitesTab.Controls.Add($siteHeader)

$sitesLabel = New-Object System.Windows.Forms.Label
$sitesLabel.Text = "Domínios separados por vírgula (ex.: exemplo.com, rede.com):"
$sitesLabel.Location = New-Object System.Drawing.Point(24, 62)
$sitesLabel.AutoSize = $true
[void]$sitesTab.Controls.Add($sitesLabel)

$sitesBox = New-Object System.Windows.Forms.TextBox
$sitesBox.Location = New-Object System.Drawing.Point(24, 89)
$sitesBox.Size = New-Object System.Drawing.Size(690, 140)
$sitesBox.Multiline = $true
$sitesBox.ScrollBars = "Vertical"
[void]$sitesTab.Controls.Add($sitesBox)

$adultSitesCheck = New-Object System.Windows.Forms.CheckBox
$adultSitesCheck.Text = "Bloquear lista padrão de sites adultos (20 domínios)"
$adultSitesCheck.Location = New-Object System.Drawing.Point(24, 245)
$adultSitesCheck.AutoSize = $true
$adultSitesCheck.Checked = $true
[void]$sitesTab.Controls.Add($adultSitesCheck)

$saveSites = New-Object System.Windows.Forms.Button
$saveSites.Text = "Salvar lista"
$saveSites.Location = New-Object System.Drawing.Point(24, 280)
$saveSites.Size = New-Object System.Drawing.Size(145, 38)
[void]$sitesTab.Controls.Add($saveSites)

$siteBlockingEnabled = $true
if ($config -and $config.PSObject.Properties.Name -contains "site_blocking_enabled") {
    $siteBlockingEnabled = [bool]$config.site_blocking_enabled
}
$releaseSites = New-Object System.Windows.Forms.Button
$releaseSites.Text = if ($siteBlockingEnabled) { "Liberar sites" } else { "Bloquear sites" }
$releaseSites.Location = New-Object System.Drawing.Point(185, 280)
$releaseSites.Size = New-Object System.Drawing.Size(145, 38)
[void]$sitesTab.Controls.Add($releaseSites)

$sitesHelp = New-Object System.Windows.Forms.Label
$sitesHelp.Text = "Liberar sites remove as regras até você clicar em Bloquear sites. Cada ação pede a senha."
$sitesHelp.Location = New-Object System.Drawing.Point(24, 330)
$sitesHelp.Size = New-Object System.Drawing.Size(690, 42)
[void]$sitesTab.Controls.Add($sitesHelp)

$siteSaveStatus = New-Object System.Windows.Forms.Label
$siteSaveStatus.Text = "Salvando e aplicando bloqueios..."
$siteSaveStatus.ForeColor = [System.Drawing.Color]::DarkBlue
$siteSaveStatus.Location = New-Object System.Drawing.Point(345, 290)
$siteSaveStatus.AutoSize = $true
$siteSaveStatus.Visible = $false
[void]$sitesTab.Controls.Add($siteSaveStatus)

if ($config) {
    if ($config.PSObject.Properties.Name -contains "adult_sites_blocked") {
        $adultSitesCheck.Checked = [bool]$config.adult_sites_blocked
    }
    $sitesBox.Text = $config.denied_sites -join ", "
}

$script:siteSaveOperation = $null
$script:siteOperationTimeoutSeconds = 45
$script:siteSaveTimer = New-Object System.Windows.Forms.Timer
$script:siteSaveTimer.Interval = 150
$script:siteSaveTimer.Add_Tick({
    if ($null -eq $script:siteSaveOperation) { return }
    $currentOperation = $script:siteSaveOperation.Current
    $process = $currentOperation.Process
    if (((Get-Date) - $currentOperation.StartedAt).TotalSeconds -ge $script:siteOperationTimeoutSeconds) {
        $script:siteSaveTimer.Stop()
        $timeoutMessage = $null
        try {
            Stop-BlocksiteAsync $currentOperation
        } catch {
            $timeoutMessage = "A operação excedeu $script:siteOperationTimeoutSeconds segundos e não foi possível encerrar o processo: $($_.Exception.Message)"
        }
        if ([string]::IsNullOrWhiteSpace($timeoutMessage)) {
            $timeoutMessage = "A operação excedeu $script:siteOperationTimeoutSeconds segundos e foi interrompida. Tente novamente; se persistir, verifique as permissões de administrador e se outro programa está usando a porta 80."
        }
        $script:siteSaveOperation = $null
        $saveSites.Enabled = $true
        $releaseSites.Enabled = $true
        $siteSaveStatus.Visible = $false
        Show-Result @{ Success = $false; Message = $timeoutMessage } "Tempo limite ao aplicar bloqueios"
        return
    }
    if (-not $process.HasExited) { return }
    if (-not $currentOperation.StdoutTask.IsCompleted -or
        -not $currentOperation.StderrTask.IsCompleted) {
        return
    }

    try {
        $result = Complete-BlocksiteAsync $currentOperation
        if ($null -eq $result) { return }
        if (-not $result.Success) {
            $script:siteSaveTimer.Stop()
            $script:siteSaveOperation = $null
            $saveSites.Enabled = $true
            $releaseSites.Enabled = $true
            $siteSaveStatus.Visible = $false
            Show-Result $result "Não foi possível salvar"
            return
        }

        if ($script:siteSaveOperation.Stage -eq "save") {
            $script:siteSaveOperation.Stage = "apply"
            $siteSaveStatus.Text = "Aplicando lista de sites..."
            $script:siteSaveOperation.Current = Start-BlocksiteAsync @("apply-sites") @($script:siteSaveOperation.Password)
            $script:siteSaveOperation.Current.StartedAt = Get-Date
            return
        }

        $script:siteSaveTimer.Stop()
        $completedOperation = $script:siteSaveOperation
        $script:siteSaveOperation = $null
        $saveSites.Enabled = $true
        $releaseSites.Enabled = $true
        $siteSaveStatus.Visible = $false
        if ($completedOperation.Stage -eq "toggle") {
            $script:siteBlockingEnabled = $completedOperation.Enabled
            $releaseSites.Text = if ($script:siteBlockingEnabled) { "Liberar sites" } else { "Bloquear sites" }
            Show-Result $result "Bloqueio de sites atualizado"
        } else {
            Show-Result $result "Lista salva e aplicada"
        }
    }
    catch {
        $script:siteSaveTimer.Stop()
        $script:siteSaveOperation = $null
        $saveSites.Enabled = $true
        $releaseSites.Enabled = $true
        $siteSaveStatus.Visible = $false
        Show-Result @{ Success = $false; Message = $_.Exception.Message } "Erro ao aplicar regras"
    }
})

$saveSites.Add_Click({
    if ($null -ne $script:siteSaveOperation) { return }
    $password = Get-PasswordOrCancel
    if ($null -eq $password) { return }
    $adultBlocked = $adultSitesCheck.Checked.ToString().ToLowerInvariant()
    try {
        $script:siteSaveOperation = @{
            Stage = "save"
            Password = $password
            Current = Start-BlocksiteAsync @("mode", "deny", $sitesBox.Text, $adultBlocked) @($password)
        }
        $script:siteSaveOperation.Current.StartedAt = Get-Date
        $saveSites.Enabled = $false
        $releaseSites.Enabled = $false
        $siteSaveStatus.Text = "Salvando e aplicando bloqueios..."
        $siteSaveStatus.Visible = $true
        $script:siteSaveTimer.Start()
    }
    catch {
        $script:siteSaveOperation = $null
        $saveSites.Enabled = $true
        $releaseSites.Enabled = $true
        $siteSaveStatus.Visible = $false
        Show-Result @{ Success = $false; Message = $_.Exception.Message } "Não foi possível salvar"
    }
})

$releaseSites.Add_Click({
    if ($null -ne $script:siteSaveOperation) { return }
    $password = Get-PasswordOrCancel
    if ($null -eq $password) { return }
    $targetEnabled = -not $script:siteBlockingEnabled
    $command = if ($targetEnabled) { "enable" } else { "disable" }
    try {
        $script:siteSaveOperation = @{
            Stage = "toggle"
            Enabled = $targetEnabled
            Password = $password
            Current = Start-BlocksiteAsync @("sites", $command) @($password)
        }
        $script:siteSaveOperation.Current.StartedAt = Get-Date
        $saveSites.Enabled = $false
        $releaseSites.Enabled = $false
        $siteSaveStatus.Text = if ($targetEnabled) { "Ativando bloqueios..." } else { "Liberando sites..." }
        $siteSaveStatus.Visible = $true
        $script:siteSaveTimer.Start()
    }
    catch {
        $script:siteSaveOperation = $null
        $saveSites.Enabled = $true
        $releaseSites.Enabled = $true
        $siteSaveStatus.Visible = $false
        Show-Result @{ Success = $false; Message = $_.Exception.Message } "Não foi possível atualizar o bloqueio"
    }
})

$wallHeader = New-Object System.Windows.Forms.Label
$wallHeader.Text = "Imagem da área de trabalho"
$wallHeader.Font = New-Object System.Drawing.Font("Segoe UI Semibold", 12)
$wallHeader.Location = New-Object System.Drawing.Point(22, 22)
$wallHeader.AutoSize = $true
[void]$wallpaperTab.Controls.Add($wallHeader)

$wallHelp = New-Object System.Windows.Forms.Label
$wallHelp.Text = "Escolha uma imagem. O Blocksite aplica o wallpaper e pode desativar a opção de troca."
$wallHelp.Location = New-Object System.Drawing.Point(24, 60)
$wallHelp.Size = New-Object System.Drawing.Size(690, 42)
[void]$wallpaperTab.Controls.Add($wallHelp)

$wallPath = New-Object System.Windows.Forms.TextBox
$wallPath.Location = New-Object System.Drawing.Point(24, 112)
$wallPath.Size = New-Object System.Drawing.Size(570, 28)
if ($config -and $config.wallpaper_path) { $wallPath.Text = $config.wallpaper_path }
[void]$wallpaperTab.Controls.Add($wallPath)

$browse = New-Object System.Windows.Forms.Button
$browse.Text = "Procurar..."
$browse.Location = New-Object System.Drawing.Point(605, 109)
$browse.Size = New-Object System.Drawing.Size(110, 32)
[void]$wallpaperTab.Controls.Add($browse)

$importWallpaper = New-Object System.Windows.Forms.Button
$importWallpaper.Text = "Aplicar e bloquear wallpaper"
$importWallpaper.Location = New-Object System.Drawing.Point(24, 165)
$importWallpaper.Size = New-Object System.Drawing.Size(230, 38)
[void]$wallpaperTab.Controls.Add($importWallpaper)

$unlockWallpaper = New-Object System.Windows.Forms.Button
$unlockWallpaper.Text = "Liberar troca do wallpaper"
$unlockWallpaper.Location = New-Object System.Drawing.Point(24, 220)
$unlockWallpaper.Size = New-Object System.Drawing.Size(230, 38)
[void]$wallpaperTab.Controls.Add($unlockWallpaper)

$browse.Add_Click({
    $picker = New-Object System.Windows.Forms.OpenFileDialog
    $picker.Filter = "Imagens|*.png;*.jpg;*.jpeg;*.bmp|Todos os arquivos|*.*"
    if ($picker.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
        $wallPath.Text = $picker.FileName
    }
    $picker.Dispose()
})

$importWallpaper.Add_Click({
    [void](Invoke-ProtectedCommand @("wallpaper", $wallPath.Text) "Wallpaper")
})
$unlockWallpaper.Add_Click({
    [void](Invoke-ProtectedCommand @("unlock-wallpaper") "Wallpaper")
})

$permissionsHeader = New-Object System.Windows.Forms.Label
$permissionsHeader.Text = "Liberações temporárias"
$permissionsHeader.Font = New-Object System.Drawing.Font("Segoe UI Semibold", 12)
$permissionsHeader.Location = New-Object System.Drawing.Point(22, 22)
$permissionsHeader.AutoSize = $true
[void]$settingsTab.Controls.Add($permissionsHeader)

$installHelp = New-Object System.Windows.Forms.Label
$installHelp.Text = "A liberação dura 30 minutos. O bloqueio controla instaladores MSI; não bloqueia todos os instaladores .exe."
$installHelp.Location = New-Object System.Drawing.Point(24, 60)
$installHelp.Size = New-Object System.Drawing.Size(690, 46)
[void]$settingsTab.Controls.Add($installHelp)

$unlockInstall = New-Object System.Windows.Forms.Button
$unlockInstall.Text = "Liberar instalação e configurações por 30 min"
$unlockInstall.Location = New-Object System.Drawing.Point(24, 120)
$unlockInstall.Size = New-Object System.Drawing.Size(270, 40)
[void]$settingsTab.Controls.Add($unlockInstall)

$lockInstall = New-Object System.Windows.Forms.Button
$lockInstall.Text = "Bloquear instalação MSI agora"
$lockInstall.Location = New-Object System.Drawing.Point(310, 120)
$lockInstall.Size = New-Object System.Drawing.Size(270, 40)
[void]$settingsTab.Controls.Add($lockInstall)

$unlockSettings = New-Object System.Windows.Forms.Button
$unlockSettings.Text = "Liberar configurações por 30 min"
$unlockSettings.Location = New-Object System.Drawing.Point(24, 182)
$unlockSettings.Size = New-Object System.Drawing.Size(270, 40)
[void]$settingsTab.Controls.Add($unlockSettings)

$lockSettings = New-Object System.Windows.Forms.Button
$lockSettings.Text = "Bloquear configurações agora"
$lockSettings.Location = New-Object System.Drawing.Point(310, 182)
$lockSettings.Size = New-Object System.Drawing.Size(270, 40)
[void]$settingsTab.Controls.Add($lockSettings)

$changePassword = New-Object System.Windows.Forms.Button
$changePassword.Text = "Alterar senha"
$changePassword.Location = New-Object System.Drawing.Point(24, 250)
$changePassword.Size = New-Object System.Drawing.Size(180, 38)
[void]$settingsTab.Controls.Add($changePassword)

$lockInstall.Add_Click({ [void](Invoke-ProtectedCommand @("lock-install") "Instalação") })
$unlockInstall.Add_Click({ [void](Invoke-ProtectedCommand @("unlock-install") "Instalação") })
$lockSettings.Add_Click({ [void](Invoke-ProtectedCommand @("lock-settings") "Configurações") })
$unlockSettings.Add_Click({ [void](Invoke-ProtectedCommand @("unlock-settings") "Configurações") })
$changePassword.Add_Click({
    $values = Get-PasswordInputs @("Senha atual:", "Nova senha (mínimo 8 caracteres):", "Confirme a nova senha:")
    if ($null -ne $values) {
        Show-Result (Invoke-Blocksite @("set-password") $values) "Alteração de senha"
    }
})

$footer = New-Object System.Windows.Forms.Label
$footer.Text = "Dica: execute como administrador para aplicar políticas do Windows."
$footer.ForeColor = [System.Drawing.Color]::DimGray
$footer.Location = New-Object System.Drawing.Point(25, 566)
$footer.AutoSize = $true
[void]$form.Controls.Add($footer)

$trayMenu = New-Object System.Windows.Forms.ContextMenuStrip
$openMenuItem = New-Object System.Windows.Forms.ToolStripMenuItem("Abrir Blocksite")
$exitMenuItem = New-Object System.Windows.Forms.ToolStripMenuItem("Sair")
[void]$trayMenu.Items.Add($openMenuItem)
[void]$trayMenu.Items.Add((New-Object System.Windows.Forms.ToolStripSeparator))
[void]$trayMenu.Items.Add($exitMenuItem)

$trayIcon = New-Object System.Windows.Forms.NotifyIcon
$trayIcon.Text = "Blocksite - proteção ativa"
$trayIcon.Icon = $appIcon
$trayIcon.ContextMenuStrip = $trayMenu
$trayIcon.Visible = $true

$openWindow = {
    if ($form.Visible) {
        $form.Activate()
        return
    }

    $password = Get-PasswordOrCancel
    if ($null -eq $password) { return }
    $verification = Invoke-Blocksite @("verify-password") @($password)
    if (-not $verification.Success) {
        Show-Result $verification "Senha incorreta"
        return
    }

    $form.ShowInTaskbar = $true
    $form.Show()
    $form.WindowState = [System.Windows.Forms.FormWindowState]::Normal
    $form.Activate()
}
$openMenuItem.Add_Click({ & $openWindow })
$trayIcon.Add_MouseClick({
    param($sender, $eventArgs)
    if ($eventArgs.Button -eq [System.Windows.Forms.MouseButtons]::Left) {
        & $openWindow
    }
})

$form.Add_Resize({
    if ($form.WindowState -eq [System.Windows.Forms.FormWindowState]::Minimized) {
        $form.ShowInTaskbar = $false
        $form.Hide()
    }
})

$form.Add_FormClosing({
    param($sender, $eventArgs)
    if (-not $script:ExitConfirmed) {
        $eventArgs.Cancel = $true
        $form.ShowInTaskbar = $false
        $form.Hide()
        $trayIcon.ShowBalloonTip(
            2500,
            "Blocksite continua ativo",
            "O aplicativo foi minimizado para a área de notificação.",
            [System.Windows.Forms.ToolTipIcon]::Info
        )
    }
})

$exitMenuItem.Add_Click({
    $password = Get-PasswordOrCancel
    if ($null -eq $password) { return }
    $verification = Invoke-Blocksite @("verify-password") @($password)
    if (-not $verification.Success) {
        Show-Result $verification "Senha incorreta"
        return
    }

    $script:ExitConfirmed = $true
    $trayIcon.Visible = $false
    $trayIcon.Dispose()
    $trayMenu.Dispose()
    $form.Close()
})

[System.Windows.Forms.Application]::Run($form)
