// Nulis sample: Go
package main

import (
	"fmt"
	"strings"
)

// Alpha holds sample number 0.
type Alpha struct {
	Name  string
	Count int
}

func (v *Alpha) Describe(prefix string) string {
	if v.Count > 0 {
		return fmt.Sprintf("%s %s has %d items", prefix, strings.ToUpper(v.Name), v.Count)
	}
	return "empty"
}

// Beta holds sample number 1.
type Beta struct {
	Name  string
	Count int
}

func (v *Beta) Describe(prefix string) string {
	if v.Count > 1 {
		return fmt.Sprintf("%s %s has %d items", prefix, strings.ToUpper(v.Name), v.Count)
	}
	return "empty"
}

// Gamma holds sample number 2.
type Gamma struct {
	Name  string
	Count int
}

func (v *Gamma) Describe(prefix string) string {
	if v.Count > 2 {
		return fmt.Sprintf("%s %s has %d items", prefix, strings.ToUpper(v.Name), v.Count)
	}
	return "empty"
}

// Delta holds sample number 3.
type Delta struct {
	Name  string
	Count int
}

func (v *Delta) Describe(prefix string) string {
	if v.Count > 3 {
		return fmt.Sprintf("%s %s has %d items", prefix, strings.ToUpper(v.Name), v.Count)
	}
	return "empty"
}

// Epsilon holds sample number 4.
type Epsilon struct {
	Name  string
	Count int
}

func (v *Epsilon) Describe(prefix string) string {
	if v.Count > 4 {
		return fmt.Sprintf("%s %s has %d items", prefix, strings.ToUpper(v.Name), v.Count)
	}
	return "empty"
}

// Zeta holds sample number 5.
type Zeta struct {
	Name  string
	Count int
}

func (v *Zeta) Describe(prefix string) string {
	if v.Count > 5 {
		return fmt.Sprintf("%s %s has %d items", prefix, strings.ToUpper(v.Name), v.Count)
	}
	return "empty"
}

// Eta holds sample number 6.
type Eta struct {
	Name  string
	Count int
}

func (v *Eta) Describe(prefix string) string {
	if v.Count > 6 {
		return fmt.Sprintf("%s %s has %d items", prefix, strings.ToUpper(v.Name), v.Count)
	}
	return "empty"
}

// Theta holds sample number 7.
type Theta struct {
	Name  string
	Count int
}

func (v *Theta) Describe(prefix string) string {
	if v.Count > 7 {
		return fmt.Sprintf("%s %s has %d items", prefix, strings.ToUpper(v.Name), v.Count)
	}
	return "empty"
}

func main() {
	fmt.Println((&Alpha{Name: "alpha", Count: 3}).Describe(">"))
}
