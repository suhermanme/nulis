-- Nulis sample: AppleScript
(* Block comments look like this. *)
property appName : "Nulis"
set counter to 0

on processAlpha(itemCount)
	-- Handler number 0
	set total to itemCount * 2
	if total > 0 then
		display dialog "Total for alpha: " & total
	else
		log "small value " & (total as string)
	end if
	return total
end processAlpha

on processBeta(itemCount)
	-- Handler number 1
	set total to itemCount * 3
	if total > 10 then
		display dialog "Total for beta: " & total
	else
		log "small value " & (total as string)
	end if
	return total
end processBeta

on processGamma(itemCount)
	-- Handler number 2
	set total to itemCount * 4
	if total > 20 then
		display dialog "Total for gamma: " & total
	else
		log "small value " & (total as string)
	end if
	return total
end processGamma

on processDelta(itemCount)
	-- Handler number 3
	set total to itemCount * 5
	if total > 30 then
		display dialog "Total for delta: " & total
	else
		log "small value " & (total as string)
	end if
	return total
end processDelta

on processEpsilon(itemCount)
	-- Handler number 4
	set total to itemCount * 6
	if total > 40 then
		display dialog "Total for epsilon: " & total
	else
		log "small value " & (total as string)
	end if
	return total
end processEpsilon

on processZeta(itemCount)
	-- Handler number 5
	set total to itemCount * 7
	if total > 50 then
		display dialog "Total for zeta: " & total
	else
		log "small value " & (total as string)
	end if
	return total
end processZeta

on processEta(itemCount)
	-- Handler number 6
	set total to itemCount * 8
	if total > 60 then
		display dialog "Total for eta: " & total
	else
		log "small value " & (total as string)
	end if
	return total
end processEta

on processTheta(itemCount)
	-- Handler number 7
	set total to itemCount * 9
	if total > 70 then
		display dialog "Total for theta: " & total
	else
		log "small value " & (total as string)
	end if
	return total
end processTheta

on processIota(itemCount)
	-- Handler number 8
	set total to itemCount * 10
	if total > 80 then
		display dialog "Total for iota: " & total
	else
		log "small value " & (total as string)
	end if
	return total
end processIota

on processKappa(itemCount)
	-- Handler number 9
	set total to itemCount * 11
	if total > 90 then
		display dialog "Total for kappa: " & total
	else
		log "small value " & (total as string)
	end if
	return total
end processKappa
