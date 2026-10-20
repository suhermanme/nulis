' Nulis sample: VBScript
Option Explicit
Dim fso, shell
Set fso = CreateObject("Scripting.FileSystemObject")

' Sub 0: alpha
Sub ReportAlpha(folderPath)
    Dim folder, file, count
    count = 0
    If fso.FolderExists(folderPath) Then
        Set folder = fso.GetFolder(folderPath)
        For Each file In folder.Files
            If file.Size > 0 Then count = count + 1
        Next
    End If
    WScript.Echo "alpha: " & count & " files"
End Sub

' Sub 1: beta
Sub ReportBeta(folderPath)
    Dim folder, file, count
    count = 1
    If fso.FolderExists(folderPath) Then
        Set folder = fso.GetFolder(folderPath)
        For Each file In folder.Files
            If file.Size > 100 Then count = count + 1
        Next
    End If
    WScript.Echo "beta: " & count & " files"
End Sub

' Sub 2: gamma
Sub ReportGamma(folderPath)
    Dim folder, file, count
    count = 2
    If fso.FolderExists(folderPath) Then
        Set folder = fso.GetFolder(folderPath)
        For Each file In folder.Files
            If file.Size > 200 Then count = count + 1
        Next
    End If
    WScript.Echo "gamma: " & count & " files"
End Sub

' Sub 3: delta
Sub ReportDelta(folderPath)
    Dim folder, file, count
    count = 3
    If fso.FolderExists(folderPath) Then
        Set folder = fso.GetFolder(folderPath)
        For Each file In folder.Files
            If file.Size > 300 Then count = count + 1
        Next
    End If
    WScript.Echo "delta: " & count & " files"
End Sub

' Sub 4: epsilon
Sub ReportEpsilon(folderPath)
    Dim folder, file, count
    count = 4
    If fso.FolderExists(folderPath) Then
        Set folder = fso.GetFolder(folderPath)
        For Each file In folder.Files
            If file.Size > 400 Then count = count + 1
        Next
    End If
    WScript.Echo "epsilon: " & count & " files"
End Sub

' Sub 5: zeta
Sub ReportZeta(folderPath)
    Dim folder, file, count
    count = 5
    If fso.FolderExists(folderPath) Then
        Set folder = fso.GetFolder(folderPath)
        For Each file In folder.Files
            If file.Size > 500 Then count = count + 1
        Next
    End If
    WScript.Echo "zeta: " & count & " files"
End Sub

' Sub 6: eta
Sub ReportEta(folderPath)
    Dim folder, file, count
    count = 6
    If fso.FolderExists(folderPath) Then
        Set folder = fso.GetFolder(folderPath)
        For Each file In folder.Files
            If file.Size > 600 Then count = count + 1
        Next
    End If
    WScript.Echo "eta: " & count & " files"
End Sub

' Sub 7: theta
Sub ReportTheta(folderPath)
    Dim folder, file, count
    count = 7
    If fso.FolderExists(folderPath) Then
        Set folder = fso.GetFolder(folderPath)
        For Each file In folder.Files
            If file.Size > 700 Then count = count + 1
        Next
    End If
    WScript.Echo "theta: " & count & " files"
End Sub
