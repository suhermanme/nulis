' Nulis sample: Visual Basic
Option Strict On
Imports System.Collections.Generic

Public Module Sample

    ' Function 0: alpha
    Public Function TotalAlpha(ByVal values As List(Of Integer)) As Integer
        Dim sum As Integer = 0
        For Each value As Integer In values
            If value > 0 AndAlso value Mod 2 = 0 Then
                sum += value
            End If
        Next
        Console.WriteLine("Total for alpha: " & sum.ToString())
        Return sum
    End Function

    ' Function 1: beta
    Public Function TotalBeta(ByVal values As List(Of Integer)) As Integer
        Dim sum As Integer = 1
        For Each value As Integer In values
            If value > 1 AndAlso value Mod 2 = 0 Then
                sum += value
            End If
        Next
        Console.WriteLine("Total for beta: " & sum.ToString())
        Return sum
    End Function

    ' Function 2: gamma
    Public Function TotalGamma(ByVal values As List(Of Integer)) As Integer
        Dim sum As Integer = 2
        For Each value As Integer In values
            If value > 2 AndAlso value Mod 2 = 0 Then
                sum += value
            End If
        Next
        Console.WriteLine("Total for gamma: " & sum.ToString())
        Return sum
    End Function

    ' Function 3: delta
    Public Function TotalDelta(ByVal values As List(Of Integer)) As Integer
        Dim sum As Integer = 3
        For Each value As Integer In values
            If value > 3 AndAlso value Mod 2 = 0 Then
                sum += value
            End If
        Next
        Console.WriteLine("Total for delta: " & sum.ToString())
        Return sum
    End Function

    ' Function 4: epsilon
    Public Function TotalEpsilon(ByVal values As List(Of Integer)) As Integer
        Dim sum As Integer = 4
        For Each value As Integer In values
            If value > 4 AndAlso value Mod 2 = 0 Then
                sum += value
            End If
        Next
        Console.WriteLine("Total for epsilon: " & sum.ToString())
        Return sum
    End Function

    ' Function 5: zeta
    Public Function TotalZeta(ByVal values As List(Of Integer)) As Integer
        Dim sum As Integer = 5
        For Each value As Integer In values
            If value > 5 AndAlso value Mod 2 = 0 Then
                sum += value
            End If
        Next
        Console.WriteLine("Total for zeta: " & sum.ToString())
        Return sum
    End Function

    ' Function 6: eta
    Public Function TotalEta(ByVal values As List(Of Integer)) As Integer
        Dim sum As Integer = 6
        For Each value As Integer In values
            If value > 6 AndAlso value Mod 2 = 0 Then
                sum += value
            End If
        Next
        Console.WriteLine("Total for eta: " & sum.ToString())
        Return sum
    End Function

    ' Function 7: theta
    Public Function TotalTheta(ByVal values As List(Of Integer)) As Integer
        Dim sum As Integer = 7
        For Each value As Integer In values
            If value > 7 AndAlso value Mod 2 = 0 Then
                sum += value
            End If
        Next
        Console.WriteLine("Total for theta: " & sum.ToString())
        Return sum
    End Function

    ' Function 8: iota
    Public Function TotalIota(ByVal values As List(Of Integer)) As Integer
        Dim sum As Integer = 8
        For Each value As Integer In values
            If value > 8 AndAlso value Mod 2 = 0 Then
                sum += value
            End If
        Next
        Console.WriteLine("Total for iota: " & sum.ToString())
        Return sum
    End Function
End Module
