@echo off
REM Nulis sample: Batch
setlocal enabledelayedexpansion
set "ROOT=%~dp0"

:step_alpha
REM Step 0 copies the alpha files
set /a COUNT=0+1
if exist "%ROOT%alpha\*.txt" (
    echo Copying alpha files: !COUNT!
    xcopy /y /q "%ROOT%alpha\*.txt" "%TEMP%\alpha\" >nul
) else (
    echo No files for alpha
)

:step_beta
REM Step 1 copies the beta files
set /a COUNT=1+1
if exist "%ROOT%beta\*.txt" (
    echo Copying beta files: !COUNT!
    xcopy /y /q "%ROOT%beta\*.txt" "%TEMP%\beta\" >nul
) else (
    echo No files for beta
)

:step_gamma
REM Step 2 copies the gamma files
set /a COUNT=2+1
if exist "%ROOT%gamma\*.txt" (
    echo Copying gamma files: !COUNT!
    xcopy /y /q "%ROOT%gamma\*.txt" "%TEMP%\gamma\" >nul
) else (
    echo No files for gamma
)

:step_delta
REM Step 3 copies the delta files
set /a COUNT=3+1
if exist "%ROOT%delta\*.txt" (
    echo Copying delta files: !COUNT!
    xcopy /y /q "%ROOT%delta\*.txt" "%TEMP%\delta\" >nul
) else (
    echo No files for delta
)

:step_epsilon
REM Step 4 copies the epsilon files
set /a COUNT=4+1
if exist "%ROOT%epsilon\*.txt" (
    echo Copying epsilon files: !COUNT!
    xcopy /y /q "%ROOT%epsilon\*.txt" "%TEMP%\epsilon\" >nul
) else (
    echo No files for epsilon
)

:step_zeta
REM Step 5 copies the zeta files
set /a COUNT=5+1
if exist "%ROOT%zeta\*.txt" (
    echo Copying zeta files: !COUNT!
    xcopy /y /q "%ROOT%zeta\*.txt" "%TEMP%\zeta\" >nul
) else (
    echo No files for zeta
)

:step_eta
REM Step 6 copies the eta files
set /a COUNT=6+1
if exist "%ROOT%eta\*.txt" (
    echo Copying eta files: !COUNT!
    xcopy /y /q "%ROOT%eta\*.txt" "%TEMP%\eta\" >nul
) else (
    echo No files for eta
)

:step_theta
REM Step 7 copies the theta files
set /a COUNT=7+1
if exist "%ROOT%theta\*.txt" (
    echo Copying theta files: !COUNT!
    xcopy /y /q "%ROOT%theta\*.txt" "%TEMP%\theta\" >nul
) else (
    echo No files for theta
)

:step_iota
REM Step 8 copies the iota files
set /a COUNT=8+1
if exist "%ROOT%iota\*.txt" (
    echo Copying iota files: !COUNT!
    xcopy /y /q "%ROOT%iota\*.txt" "%TEMP%\iota\" >nul
) else (
    echo No files for iota
)

:step_kappa
REM Step 9 copies the kappa files
set /a COUNT=9+1
if exist "%ROOT%kappa\*.txt" (
    echo Copying kappa files: !COUNT!
    xcopy /y /q "%ROOT%kappa\*.txt" "%TEMP%\kappa\" >nul
) else (
    echo No files for kappa
)
goto :eof
