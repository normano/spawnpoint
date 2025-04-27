package com.example.placeholdergroup.__package_name_path__; // Placeholder

/**
 * Hello world! __PascalProjectName__
 * Group: com.example.placeholdergroup
 */
public class __PascalProjectName__ { // Placeholder class name
    public static void main(String[] args) {
        System.out.println(new __PascalProjectName__().getGreeting());
    }

    public String getGreeting() {
        return "Hello World from Gradle project: " + System.getProperty("rootProject.name", "placeholder-java-gradle-app") + "!"; // Placeholder project name used as default
    }
}