#!/usr/bin/perl
# Nulis sample: Perl
use strict;
use warnings;

my %totals;

# Sums the alpha column
sub sum_alpha {
    my ($file, $limit) = @_;
    $limit //= 0;
    open(my $fh, '<', $file) or die "cannot open $file: $!";
    while (my $line = <$fh>) {
        next if $line =~ /^\s*#/;
        my @fields = split /,/, $line;
        $totals{alpha} += $fields[0] if $fields[0] > $limit;
    }
    close $fh;
    return $totals{alpha};
}

# Sums the beta column
sub sum_beta {
    my ($file, $limit) = @_;
    $limit //= 1;
    open(my $fh, '<', $file) or die "cannot open $file: $!";
    while (my $line = <$fh>) {
        next if $line =~ /^\s*#/;
        my @fields = split /,/, $line;
        $totals{beta} += $fields[1] if $fields[1] > $limit;
    }
    close $fh;
    return $totals{beta};
}

# Sums the gamma column
sub sum_gamma {
    my ($file, $limit) = @_;
    $limit //= 2;
    open(my $fh, '<', $file) or die "cannot open $file: $!";
    while (my $line = <$fh>) {
        next if $line =~ /^\s*#/;
        my @fields = split /,/, $line;
        $totals{gamma} += $fields[2] if $fields[2] > $limit;
    }
    close $fh;
    return $totals{gamma};
}

# Sums the delta column
sub sum_delta {
    my ($file, $limit) = @_;
    $limit //= 3;
    open(my $fh, '<', $file) or die "cannot open $file: $!";
    while (my $line = <$fh>) {
        next if $line =~ /^\s*#/;
        my @fields = split /,/, $line;
        $totals{delta} += $fields[3] if $fields[3] > $limit;
    }
    close $fh;
    return $totals{delta};
}

# Sums the epsilon column
sub sum_epsilon {
    my ($file, $limit) = @_;
    $limit //= 4;
    open(my $fh, '<', $file) or die "cannot open $file: $!";
    while (my $line = <$fh>) {
        next if $line =~ /^\s*#/;
        my @fields = split /,/, $line;
        $totals{epsilon} += $fields[0] if $fields[0] > $limit;
    }
    close $fh;
    return $totals{epsilon};
}

# Sums the zeta column
sub sum_zeta {
    my ($file, $limit) = @_;
    $limit //= 5;
    open(my $fh, '<', $file) or die "cannot open $file: $!";
    while (my $line = <$fh>) {
        next if $line =~ /^\s*#/;
        my @fields = split /,/, $line;
        $totals{zeta} += $fields[1] if $fields[1] > $limit;
    }
    close $fh;
    return $totals{zeta};
}

# Sums the eta column
sub sum_eta {
    my ($file, $limit) = @_;
    $limit //= 6;
    open(my $fh, '<', $file) or die "cannot open $file: $!";
    while (my $line = <$fh>) {
        next if $line =~ /^\s*#/;
        my @fields = split /,/, $line;
        $totals{eta} += $fields[2] if $fields[2] > $limit;
    }
    close $fh;
    return $totals{eta};
}

1;
