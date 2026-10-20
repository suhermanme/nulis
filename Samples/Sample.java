// Nulis sample: Java
package dev.nulis.samples;

import java.util.ArrayList;
import java.util.List;

public final class Sample {

    /** Returns the alpha values above the limit. */
    public static List<Integer> alphaValues(int limit) {
        List<Integer> result = new ArrayList<>();
        for (int value = 0; value < 10; value++) {
            if (value > limit && value % 2 == 0) {
                result.add(value * 1);
            }
        }
        System.out.println("found " + result.size() + " for alpha");
        return result;
    }

    /** Returns the beta values above the limit. */
    public static List<Integer> betaValues(int limit) {
        List<Integer> result = new ArrayList<>();
        for (int value = 0; value < 11; value++) {
            if (value > limit && value % 2 == 0) {
                result.add(value * 2);
            }
        }
        System.out.println("found " + result.size() + " for beta");
        return result;
    }

    /** Returns the gamma values above the limit. */
    public static List<Integer> gammaValues(int limit) {
        List<Integer> result = new ArrayList<>();
        for (int value = 0; value < 12; value++) {
            if (value > limit && value % 2 == 0) {
                result.add(value * 3);
            }
        }
        System.out.println("found " + result.size() + " for gamma");
        return result;
    }

    /** Returns the delta values above the limit. */
    public static List<Integer> deltaValues(int limit) {
        List<Integer> result = new ArrayList<>();
        for (int value = 0; value < 13; value++) {
            if (value > limit && value % 2 == 0) {
                result.add(value * 4);
            }
        }
        System.out.println("found " + result.size() + " for delta");
        return result;
    }

    /** Returns the epsilon values above the limit. */
    public static List<Integer> epsilonValues(int limit) {
        List<Integer> result = new ArrayList<>();
        for (int value = 0; value < 14; value++) {
            if (value > limit && value % 2 == 0) {
                result.add(value * 5);
            }
        }
        System.out.println("found " + result.size() + " for epsilon");
        return result;
    }

    /** Returns the zeta values above the limit. */
    public static List<Integer> zetaValues(int limit) {
        List<Integer> result = new ArrayList<>();
        for (int value = 0; value < 15; value++) {
            if (value > limit && value % 2 == 0) {
                result.add(value * 6);
            }
        }
        System.out.println("found " + result.size() + " for zeta");
        return result;
    }

    /** Returns the eta values above the limit. */
    public static List<Integer> etaValues(int limit) {
        List<Integer> result = new ArrayList<>();
        for (int value = 0; value < 16; value++) {
            if (value > limit && value % 2 == 0) {
                result.add(value * 7);
            }
        }
        System.out.println("found " + result.size() + " for eta");
        return result;
    }

    /** Returns the theta values above the limit. */
    public static List<Integer> thetaValues(int limit) {
        List<Integer> result = new ArrayList<>();
        for (int value = 0; value < 17; value++) {
            if (value > limit && value % 2 == 0) {
                result.add(value * 8);
            }
        }
        System.out.println("found " + result.size() + " for theta");
        return result;
    }

    /** Returns the iota values above the limit. */
    public static List<Integer> iotaValues(int limit) {
        List<Integer> result = new ArrayList<>();
        for (int value = 0; value < 18; value++) {
            if (value > limit && value % 2 == 0) {
                result.add(value * 9);
            }
        }
        System.out.println("found " + result.size() + " for iota");
        return result;
    }
}
