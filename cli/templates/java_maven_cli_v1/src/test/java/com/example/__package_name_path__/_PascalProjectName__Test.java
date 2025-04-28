package com.example.placeholdergroup.__placeholder_package_path__; // Placeholder

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;
import org.junit.jupiter.api.Test;

/**
 * Unit test for simple PlaceholderApp.
 */
public class PlaceholderAppTest // Placeholder class name
{
    /**
     * Rigorous Test :-)
     */
    @Test
    public void shouldAnswerWithTrue()
    {
        assertTrue( true );
    }

    @Test
    public void testGetGreeting() {
        PlaceholderApp app = new PlaceholderApp(); // Placeholder class name
        assertEquals("Hello World.", app.getGreeting());
    }
}