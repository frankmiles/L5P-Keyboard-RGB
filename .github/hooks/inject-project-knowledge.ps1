$ErrorActionPreference = 'Stop'

$knowledgePath = Join-Path $PSScriptRoot '..\..\PROJECT_KNOWLEDGE.md'
if (-not (Test-Path -LiteralPath $knowledgePath -PathType Leaf)) {
    throw "Project knowledge document not found: $knowledgePath"
}

$knowledge = Get-Content -LiteralPath $knowledgePath -Raw -Encoding UTF8 -ErrorAction Stop
$message = @"
Project reference loaded from PROJECT_KNOWLEDGE.md. Use its search index to locate relevant facts and verify them against the current source when needed.

$knowledge
"@
$utf8 = New-Object System.Text.UTF8Encoding -ArgumentList $false
[Console]::OutputEncoding = $utf8
ConvertTo-Json -InputObject @{ systemMessage = $message } -Compress -Depth 3
