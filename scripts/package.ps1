#Requires -Version 7.0

[CmdletBinding()]
param(
    [ValidateSet('x64')]
    [string[]]$Arch,

    [ValidateSet('installer', 'portable', 'both')]
    [string]$PackageType = 'both',

    [string]$RuntimeDirectory,

    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot

function Invoke-Cargo {
    param([string[]]$Arguments)

    & cargo @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Cargo 命令执行失败，退出码：$LASTEXITCODE"
    }
}

function Assert-WindowsArchitecture {
    param([string]$Path, [string]$Architecture)

    $stream = [System.IO.File]::OpenRead($Path)
    try {
        $reader = [System.IO.BinaryReader]::new($stream)
        $stream.Position = 0x3c
        $peOffset = $reader.ReadInt32()
        $stream.Position = $peOffset
        $signature = $reader.ReadUInt32()
        $machine = $reader.ReadUInt16()
        $expectedMachine = if ($Architecture -eq 'x64') { 0x8664 } else { 0xaa64 }
        if ($signature -ne 0x4550 -or $machine -ne $expectedMachine) {
            throw "程序或运行库不是目标架构 $Architecture 的 PE 文件：$Path"
        }
    } finally {
        $stream.Dispose()
    }
}

function Find-WindowsRuntime {
    param([string]$Architecture)

    $directory = $RuntimeDirectory
    if (!$directory) {
        $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
        if (!(Test-Path -LiteralPath $vswhere -PathType Leaf)) {
            throw '找不到 Visual Studio Installer，请使用 -RuntimeDirectory 指定对应架构的 MSVC CRT 可再分发目录'
        }
        $installations = & $vswhere -products '*' -property installationPath
        if ($LASTEXITCODE -ne 0) {
            throw '无法查询 Visual Studio 安装目录'
        }
        $candidates = foreach ($installation in $installations) {
            $redistRoot = Join-Path $installation 'VC/Redist/MSVC'
            if (Test-Path -LiteralPath $redistRoot -PathType Container) {
                Get-ChildItem -LiteralPath $redistRoot -Directory |
                    Where-Object { $_.Name -match '^\d+\.\d+\.\d+$' } |
                    ForEach-Object {
                        $crtDirectory = Join-Path $_.FullName "$Architecture/Microsoft.VC143.CRT"
                        if (Test-Path -LiteralPath $crtDirectory -PathType Container) {
                            [pscustomobject]@{ Version = [version]$_.Name; Directory = $crtDirectory }
                        }
                    }
            }
        }
        $directory = ($candidates | Sort-Object Version -Descending | Select-Object -First 1).Directory
    }
    if (!$directory -or !(Test-Path -LiteralPath (Join-Path $directory 'vcruntime140.dll') -PathType Leaf)) {
        throw "找不到 $Architecture 的 MSVC 可再分发运行库，请安装对应 Visual Studio C++ 组件，或使用 -RuntimeDirectory 指定 Microsoft.VC143.CRT 目录"
    }
    $runtimeFiles = @(Get-Item -LiteralPath (Join-Path $directory 'vcruntime140.dll'))
    foreach ($runtimeFile in $runtimeFiles) {
        Assert-WindowsArchitecture -Path $runtimeFile.FullName -Architecture $Architecture
    }
    return $runtimeFiles
}

Push-Location -LiteralPath $projectRoot
try {
    $rustInfo = & rustc -vV
    if ($LASTEXITCODE -ne 0) {
        throw '无法读取 Rust 工具链信息'
    }
    $hostLine = $rustInfo | Select-String -Pattern '^host: (.+)$'
    if (!$hostLine) {
        throw '无法确定 Rust 工具链的主机目标'
    }
    $hostTarget = $hostLine.Matches[0].Groups[1].Value
    if ($hostTarget -notmatch '-pc-windows-msvc$') {
        throw "本项目仅支持 Windows MSVC 工具链，当前主机目标：$hostTarget"
    }

    $targetSuffix = 'pc-windows-msvc'
    $binaryName = 'quick-paste.exe'
    $packageName = 'quick-paste'

    if (!$Arch) {
        if ($hostTarget.StartsWith('x86_64-')) {
            $Arch = @('x64')
        } else {
            throw "不支持当前主机架构：$hostTarget"
        }
    }

    $createInstaller = $PackageType -ne 'portable'
    $createPortable = $PackageType -ne 'installer'
    if ($createInstaller) {
        & cargo packager --version
        if ($LASTEXITCODE -ne 0) {
            throw '请先安装打包工具：cargo install cargo-packager --version 0.11.8 --locked'
        }
    }

    if (!(Test-Path -LiteralPath (Join-Path $projectRoot 'Cargo.lock') -PathType Leaf)) {
        Invoke-Cargo -Arguments @('generate-lockfile')
    }

    $metadataJson = & cargo metadata --no-deps --format-version 1 --locked
    if ($LASTEXITCODE -ne 0) {
        throw '无法读取 Cargo 项目元数据'
    }
    $metadata = ($metadataJson -join "`n") | ConvertFrom-Json
    $package = $metadata.packages | Where-Object { $_.name -eq $packageName }
    if (!$package) {
        throw "Cargo 元数据中找不到 $packageName 项目"
    }

    foreach ($architecture in ($Arch | Select-Object -Unique)) {
        $rustArch = if ($architecture -eq 'x64') { 'x86_64' } else { 'aarch64' }
        $target = "$rustArch-$targetSuffix"
        $binaryDirectory = Join-Path $metadata.target_directory 'release'
        $buildArguments = @('build', '--release', '--locked', '--bin', $packageName)
        if ($target -ne $hostTarget) {
            $binaryDirectory = Join-Path $metadata.target_directory "$target/release"
            $buildArguments += @('--target', $target)
        }
        $outputDirectory = Join-Path $metadata.target_directory "packages/$target"

        Write-Host "正在打包 Windows / $architecture，Rust 目标：$target"
        if (!$SkipBuild) {
            Invoke-Cargo -Arguments $buildArguments
        }
        $binaryPath = Join-Path $binaryDirectory $binaryName
        if (!(Test-Path -LiteralPath $binaryPath -PathType Leaf)) {
            throw "找不到待打包程序：$binaryPath，请先构建对应目标或移除 -SkipBuild"
        }

        Assert-WindowsArchitecture -Path $binaryPath -Architecture $architecture
        $runtimeFiles = @(Find-WindowsRuntime -Architecture $architecture)

        # 直接传入配置，避免打包器忽略 --binaries-dir 后选错架构目录。
        $config = $package.metadata.packager | ConvertTo-Json -Depth 20 | ConvertFrom-Json -AsHashtable
        $config['name'] = $package.name
        $config['version'] = $package.version
        $config['binaries'] = @(@{ path = $packageName; main = $true })
        $config['binariesDir'] = $binaryDirectory
        $config['outDir'] = $outputDirectory
        $config['targetTriple'] = $target
        $config['formats'] = @('nsis')
        if ($runtimeFiles.Count -gt 0) {
            $config['resources'] = @($config['resources'] | Where-Object { $null -ne $_ }) + @($runtimeFiles.FullName)
        }

        if ($createInstaller) {
            $configJson = ConvertTo-Json -InputObject $config -Depth 20 -Compress
            Invoke-Cargo -Arguments @('packager', '--config', $configJson)
            $setup = Get-ChildItem -LiteralPath $outputDirectory -File -Filter '*-setup.exe' |
                Select-Object -First 1
            if (!$setup -or $setup.Length -eq 0) {
                throw "没有找到 NSIS 安装包：$outputDirectory"
            }
            Write-Host "安装包已生成：$($setup.FullName)"
        }

        if ($createPortable) {
            New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null
            $productName = [string]$config['product-name']
            if ([string]::IsNullOrWhiteSpace($productName)) {
                $productName = '备忘快贴'
            }
            $stageDirectory = Join-Path $outputDirectory ".portable-$([guid]::NewGuid().ToString('N'))"
            $portableRoot = Join-Path $stageDirectory $productName
            $archivePath = Join-Path $outputDirectory "${packageName}_$($package.version)_${architecture}-portable.zip"
            try {
                New-Item -ItemType Directory -Path $portableRoot | Out-Null
                Copy-Item -LiteralPath $binaryPath -Destination $portableRoot
                foreach ($runtimeFile in $runtimeFiles) {
                    Copy-Item -LiteralPath $runtimeFile.FullName -Destination $portableRoot
                }
                Copy-Item -LiteralPath (Join-Path $projectRoot 'README.md') -Destination $portableRoot
                Copy-Item -LiteralPath (Join-Path $projectRoot 'LICENSE') -Destination $portableRoot
                $assetsDirectory = Join-Path $projectRoot 'assets'
                if (Test-Path -LiteralPath $assetsDirectory -PathType Container) {
                    Copy-Item -LiteralPath $assetsDirectory -Destination $portableRoot -Recurse
                }
                Compress-Archive -LiteralPath $portableRoot -DestinationPath $archivePath -CompressionLevel Optimal -Force
                if (!(Test-Path -LiteralPath $archivePath -PathType Leaf) -or (Get-Item -LiteralPath $archivePath).Length -eq 0) {
                    throw "便携 ZIP 生成失败：$archivePath"
                }
                Write-Host "便携版已生成：$archivePath"
            } finally {
                # 只清理本次打包创建、且位于产物目录内的暂存目录。
                if (Test-Path -LiteralPath $stageDirectory -PathType Container) {
                    $resolvedOutput = (Resolve-Path -LiteralPath $outputDirectory).Path
                    $resolvedStage = (Resolve-Path -LiteralPath $stageDirectory).Path
                    if ((Split-Path -Parent $resolvedStage) -ne $resolvedOutput) {
                        throw "暂存目录不在产物目录内，拒绝清理：$resolvedStage"
                    }
                    Remove-Item -LiteralPath $resolvedStage -Recurse -Force
                }
            }
        }
        Write-Host "打包完成：$outputDirectory"
    }
} finally {
    Pop-Location
}
